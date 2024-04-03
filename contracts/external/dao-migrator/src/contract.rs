use std::{collections::HashSet, env};

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, ContractInfoResponse, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Reply,
    Response, StdResult, SubMsg, WasmMsg,
};
use dao_interface::{
    query::SubDao,
    state::{AnyContractInfo, ModuleInstantiateCallback, ProposalModule},
};
use secret_cw2::set_contract_version;

use crate::{
    error::ContractError,
    msg::{ExecuteMsg, InstantiateMsg, MigrateV1ToV2, QueryMsg},
    snip20_stake,
    state::{CORE_INFO, MODULES_ADDRS, TEST_STATE},
    types::{
        CodeIdPair, MigrationMsgs, MigrationParams, ModulesAddrs, TestState, V1CodeIdsAndHashes,
        V2CodeIdsAndHashes,
    },
    utils::state_queries::{
        query_proposal_count_v1, query_proposal_count_v2, query_proposal_v1, query_proposal_v2,
        query_total_voting_power_v1,
    },
};

pub(crate) const CONTRACT_NAME: &str = "crates.io:dao-migrator";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) const V1_V2_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    CORE_INFO.save(
        deps.storage,
        &AnyContractInfo {
            addr: info.sender,
            code_hash: msg.dao_code_hash.clone(),
        },
    )?;

    Ok(
        Response::default().set_data(to_binary(&ModuleInstantiateCallback {
            msgs: vec![WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                code_hash: env.contract.code_hash,
                msg: to_binary(&MigrateV1ToV2 {
                    sub_daos: msg.sub_daos,
                    dao_code_hash: msg.dao_code_hash,
                    migration_params: msg.migration_params,
                    v1_code_ids_and_hashes: msg.v1_code_ids_and_hashes,
                    v2_code_ids_and_hashes: msg.v2_code_ids_and_hashes,
                })?,
                funds: vec![],
            }
            .into()],
        })?),
    )
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    execute_migration_v1_v2(
        deps,
        env,
        info,
        msg.sub_daos,
        msg.migration_params,
        msg.v1_code_ids_and_hashes,
        msg.v2_code_ids_and_hashes,
    )
}

fn execute_migration_v1_v2(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sub_daos: Vec<SubDao>,
    migration_params: MigrationParams,
    v1_code_ids_and_hashes: V1CodeIdsAndHashes,
    v2_code_ids_and_hashes: V2CodeIdsAndHashes,
) -> Result<Response, ContractError> {
    if info.sender != CORE_INFO.load(deps.storage)?.addr {
        return Err(ContractError::Unauthorized {});
    }

    //Check if params doesn't have duplicates
    let mut uniq = HashSet::new();
    if !migration_params
        .proposal_params
        .iter()
        .all(|(addr, _)| uniq.insert(addr))
    {
        return Err(ContractError::DuplicateProposalParams);
    }

    // List of code ids pairs we got and the migration msg of each one of them.
    let proposal_pairs: Vec<(String, CodeIdPair)> = migration_params
        .proposal_params
        .clone()
        .into_iter()
        .map(|(addr, proposal_params)| {
            (
                addr,
                CodeIdPair::new(
                    v1_code_ids_and_hashes.proposal_single,
                    v2_code_ids_and_hashes.proposal_single,
                    MigrationMsgs::DaoProposalSingle(
                        dao_proposal_single::msg::MigrateMsg::FromV1 {
                            close_proposal_on_execution_failure: proposal_params
                                .close_proposal_on_execution_failure,
                            pre_propose_info: proposal_params.pre_propose_info,
                            veto: proposal_params.veto,
                        },
                    ),
                ),
            )
        })
        .collect(); // cw-proposal-single -> dao_proposal_single
    let voting_pairs: Vec<CodeIdPair> = vec![
        CodeIdPair::new(
            v1_code_ids_and_hashes.cw4_voting,
            v2_code_ids_and_hashes.cw4_voting,
            MigrationMsgs::DaoVotingCw4(dao_voting_cw4::msg::MigrateMsg {}),
        ), // cw4-voting -> dao_voting_cw4
        CodeIdPair::new(
            v1_code_ids_and_hashes.snip20_staked_balances_voting,
            v2_code_ids_and_hashes.snip20_staked_balances_voting,
            MigrationMsgs::DaoVotingSnip20Staked(dao_voting_snip20_staked::msg::MigrateMsg {}),
        ), // cw20-staked-balances-voting -> dao-voting-cw20-staked
    ];
    let staking_pair = CodeIdPair::new(
        v1_code_ids_and_hashes.snip20_stake,
        v2_code_ids_and_hashes.snip20_stake,
        MigrationMsgs::Snip20Stake(snip20_stake::MigrateMsg::FromV1 {}),
    ); // cw20-stake -> cw20_stake

    let mut msgs: Vec<CosmosMsg> = vec![];
    let mut modules_addrs = ModulesAddrs::default();

    // --------------------
    // verify voting module
    // --------------------
    let voting_module: AnyContractInfo = deps.querier.query_wasm_smart(
        CORE_INFO.load(deps.storage)?.code_hash,
        info.sender.clone(),
        &dao_interface::msg::QueryMsg::VotingModule {},
    )?;

    let voting_code_id = if let Ok(contract_info) =
        deps.querier
            .query::<ContractInfoResponse>(&cosmwasm_std::QueryRequest::Wasm(
                cosmwasm_std::WasmQuery::ContractInfo {
                    contract_addr: voting_module.clone().addr.to_string(),
                },
            )) {
        contract_info.code_id
    } else {
        // Return false if we don't get contract info, means something went wrong.
        return Err(ContractError::NoContractInfo {
            address: voting_module.clone().addr.into(),
        });
    };

    if let Some(voting_pair) = voting_pairs
        .into_iter()
        .find(|x| x.v1_code_id == voting_code_id)
    {
        msgs.push(
            WasmMsg::Migrate {
                contract_addr: voting_module.clone().addr.to_string(),
                code_id: voting_pair.v2_code_id,
                msg: to_binary(&voting_pair.migrate_msg).unwrap(),
                code_hash: voting_module.clone().code_hash,
            }
            .into(),
        );
        modules_addrs.voting = Some((voting_module.clone().addr, voting_module.clone().code_hash));

        // If voting module is staked cw20, we check that they confirmed migration
        // and migrate the cw20_staked module
        if let MigrationMsgs::DaoVotingSnip20Staked(_) = voting_pair.migrate_msg {
            if !migration_params
                .migrate_stake_snip20_manager
                .unwrap_or_default()
            {
                return Err(ContractError::DontMigrateSnip20);
            }

            let snip20_staked_info: AnyContractInfo = deps.querier.query_wasm_smart(
                voting_module.clone().code_hash,
                voting_module.clone().addr.to_string(),
                &dao_voting_snip20_staked::msg::QueryMsg::StakingContract {},
            )?;

            let snip20_staked_code_id = if let Ok(contract_info) = deps
                .querier
                .query::<ContractInfoResponse>(&cosmwasm_std::QueryRequest::Wasm(
                    cosmwasm_std::WasmQuery::ContractInfo {
                        contract_addr: snip20_staked_info.clone().addr.to_string(),
                    },
                )) {
                contract_info.code_id
            } else {
                // Return false if we don't get contract info, means something went wrong.
                return Err(ContractError::NoContractInfo {
                    address: snip20_staked_info.clone().addr.into(),
                });
            };

            // If module is not DAO DAO module
            if snip20_staked_code_id != staking_pair.v1_code_id {
                return Err(ContractError::CantMigrateModule {
                    code_id: snip20_staked_code_id,
                });
            }

            msgs.push(
                WasmMsg::Migrate {
                    contract_addr: snip20_staked_info.clone().addr.to_string(),
                    code_hash: snip20_staked_info.clone().code_hash,
                    code_id: staking_pair.v2_code_id,
                    msg: to_binary(&staking_pair.migrate_msg).unwrap(),
                }
                .into(),
            );
        }
    } else {
        return Err(ContractError::VotingModuleNotFound);
    }

    // -----------------------
    // verify proposal modules
    // -----------------------
    // We take all the proposal modules of the DAO.
    let proposal_modules: Vec<ProposalModule> = deps.querier.query_wasm_smart(
        CORE_INFO.load(deps.storage)?.code_hash,
        info.sender.clone(),
        &dao_interface::msg::QueryMsg::ProposalModules {
            start_after: None,
            limit: None,
        },
    )?;

    // We remove 1 because migration module is a proposal module, and we skip it.
    if proposal_modules.len() - 1 != (proposal_pairs.len()) {
        return Err(ContractError::MigrationParamsNotEqualProposalModulesLength);
    }

    // Loop over proposals and verify that they are valid DAO DAO modules
    // and set them to be migrated.
    proposal_modules
        .iter()
        .try_for_each(|module| -> Result<(), ContractError> {
            // Instead of doing 2 loops, just ignore our module, we don't care about the vec after this.
            if module.address.clone() == env.contract.address {
                return Ok(());
            }

            let proposal_pair = proposal_pairs
                .iter()
                .find(|(addr, _)| addr == module.address.clone().as_str())
                .ok_or(ContractError::ProposalModuleNotFoundInParams {
                    addr: module.address.clone().into(),
                })?
                .1
                .clone();

            // Get the code id of the module
            let proposal_code_id = if let Ok(contract_info) = deps
                .querier
                .query::<ContractInfoResponse>(&cosmwasm_std::QueryRequest::Wasm(
                    cosmwasm_std::WasmQuery::ContractInfo {
                        contract_addr: module.address.clone().to_string(),
                    },
                )) {
                Ok(contract_info.code_id)
            } else {
                // Return false if we don't get contract info, means something went wrong.
                Err(ContractError::NoContractInfo {
                    address: module.address.clone().into(),
                })
            }?;

            // check if Code id is valid DAO DAO code id
            if proposal_code_id == proposal_pair.v1_code_id {
                msgs.push(
                    WasmMsg::Migrate {
                        contract_addr: module.address.clone().to_string(),
                        code_hash: module.code_hash.clone(),
                        code_id: proposal_pair.v2_code_id,
                        msg: to_binary(&proposal_pair.migrate_msg).unwrap(),
                    }
                    .into(),
                );
                modules_addrs
                    .proposals
                    .push((module.address.clone(), module.code_hash.clone()));
                Ok(())
            } else {
                // Return false because we couldn't find the code id on our list.
                Err(ContractError::CantMigrateModule {
                    code_id: proposal_code_id,
                })
            }?;

            Ok(())
        })?;

    // We successfully verified all modules of the DAO, we can send migration msgs.

    // Verify we got voting address, and at least 1 proposal single address
    modules_addrs.verify()?;
    MODULES_ADDRS.save(deps.storage, &modules_addrs)?;
    // Do the state query, and save it in storage
    let state = query_state_v1(deps.as_ref(), modules_addrs)?;
    TEST_STATE.save(deps.storage, &state)?;

    // Add sub daos to core
    msgs.push(
        WasmMsg::Execute {
            contract_addr: info.sender.to_string(),
            code_hash: CORE_INFO.load(deps.storage)?.code_hash,
            msg: to_binary(&dao_interface::msg::ExecuteMsg::UpdateSubDaos {
                to_add: sub_daos,
                to_remove: vec![],
            })?,
            funds: vec![],
        }
        .into(),
    );

    // Create the ExecuteProposalHook msg.
    let proposal_hook_msg = SubMsg::reply_on_success(
        WasmMsg::Execute {
            contract_addr: info.sender.to_string(),
            code_hash: CORE_INFO.load(deps.storage)?.code_hash,
            msg: to_binary(&dao_interface::msg::ExecuteMsg::ExecuteProposalHook { msgs })?,
            funds: vec![],
        },
        V1_V2_REPLY_ID,
    );

    Ok(Response::default().add_submessage(proposal_hook_msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {}
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, reply: Reply) -> Result<Response, ContractError> {
    match reply.id {
        V1_V2_REPLY_ID => {
            let core_info = CORE_INFO.load(deps.storage)?;
            // This is called after we got all the migrations successfully
            test_state(deps.as_ref())?;

            // FINALLY remove the migrator from the core
            // Reason we do it now, is because we first need to test the state
            // and only then delete our module if everything worked out.
            let remove_msg = WasmMsg::Execute {
                contract_addr: core_info.addr.clone().to_string(),
                code_hash: core_info.code_hash.clone(),
                msg: to_binary(&dao_interface::msg::ExecuteMsg::ExecuteProposalHook {
                    msgs: vec![WasmMsg::Execute {
                        contract_addr: core_info.addr.clone().to_string(),
                        code_hash: core_info.code_hash.clone(),
                        msg: to_binary(&dao_interface::msg::ExecuteMsg::UpdateProposalModules {
                            to_add: vec![],
                            to_disable: vec![env.contract.address.to_string()],
                        })?,
                        funds: vec![],
                    }
                    .into()],
                })?,
                funds: vec![],
            };

            Ok(Response::default()
                .add_message(remove_msg)
                .add_attribute("action", "migrate")
                .add_attribute("status", "success"))
        }
        _ => Err(ContractError::UnrecognisedReplyId),
    }
}

fn query_state_v1(deps: Deps, module_addrs: ModulesAddrs) -> Result<TestState, ContractError> {
    let proposal_counts = query_proposal_count_v1(deps, module_addrs.proposals.clone())?;
    let (proposals, sample_proposal_data) = query_proposal_v1(deps, module_addrs.proposals)?;
    let (voting_addr, voting_code_hash) = module_addrs.voting.unwrap();
    let total_voting_power = query_total_voting_power_v1(
        deps,
        voting_addr.clone(),
        voting_code_hash.clone(),
        sample_proposal_data.start_height,
    )?;

    Ok(TestState {
        proposal_counts,
        proposals,
        total_voting_power,
    })
}

fn query_state_v2(deps: Deps, module_addrs: ModulesAddrs) -> Result<TestState, ContractError> {
    let proposal_counts = query_proposal_count_v2(deps, module_addrs.proposals.clone())?;
    let (proposals, sample_proposal_data) =
        query_proposal_v2(deps, module_addrs.proposals.clone())?;
    let (voting_addr, voting_code_hash) = module_addrs.voting.unwrap();
    let total_voting_power = query_total_voting_power_v1(
        deps,
        voting_addr.clone(),
        voting_code_hash.clone(),
        sample_proposal_data.start_height,
    )?;

    Ok(TestState {
        proposal_counts,
        proposals,
        total_voting_power,
    })
}

fn test_state(deps: Deps) -> Result<(), ContractError> {
    let old_state = TEST_STATE.load(deps.storage)?;
    let modules_addrs = MODULES_ADDRS.load(deps.storage)?;
    let new_state = query_state_v2(deps, modules_addrs)?;

    if new_state == old_state {
        Ok(())
    } else {
        Err(ContractError::TestFailed)
    }
}
