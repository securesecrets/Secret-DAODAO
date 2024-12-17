use cosmwasm_std::{
    testing::mock_info, to_binary, Addr, Coin, ContractInfo, Decimal, Empty, Uint128,
};

use dao_interface::{
    msg::InitialBalance,
    state::{Admin, AnyContractInfo, ModuleInstantiateInfo},
};
use dao_pre_propose_single as cppbps;
use secret_multi_test::{next_block, App, BankSudo, Executor, SudoMsg};
use secret_utils::Duration;

use dao_voting::{
    deposit::{DepositRefundPolicy, UncheckedDepositInfo, VotingModuleTokenType},
    pre_propose::PreProposeInfo,
    threshold::{ActiveThreshold, PercentageThreshold, Threshold::ThresholdQuorum},
};
use dao_voting_cw4::msg::GroupContract;

use crate::msg::InstantiateMsg;

use super::{
    contracts::{
        cw4_group_contract, cw4_voting_contract, cw_core_contract,
        native_staked_balances_voting_contract, proposal_single_contract, query_auth_contract,
        snip20_base_contract, snip20_stake_contract, snip20_staked_balances_voting_contract,
    },
    execute::create_viewing_key,
    CREATOR_ADDR,
};

pub(crate) fn get_pre_propose_info(
    app: &mut App,
    deposit_info: Option<UncheckedDepositInfo>,
    open_proposal_submission: bool,
) -> PreProposeInfo {
    let pre_propose_contract =
        app.store_code(crate::testing::contracts::pre_propose_single_contract());
    PreProposeInfo::ModuleMayPropose {
        info: ModuleInstantiateInfo {
            code_id: pre_propose_contract.code_id,
            code_hash: pre_propose_contract.code_hash,
            msg: to_binary(&cppbps::InstantiateMsg {
                deposit_info,
                open_proposal_submission,
                extension: Empty::default(),
            })
            .unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "pre_propose_contract".to_string(),
        },
    }
}

pub(crate) fn get_default_token_dao_proposal_module_instantiate(app: &mut App) -> InstantiateMsg {
    InstantiateMsg {
        veto: None,
        threshold: ThresholdQuorum {
            quorum: PercentageThreshold::Percent(Decimal::percent(15)),
            threshold: PercentageThreshold::Majority {},
        },
        max_voting_period: Duration::Time(604800), // One week.
        min_voting_period: None,
        only_members_execute: true,
        allow_revoting: false,
        pre_propose_info: get_pre_propose_info(
            app,
            Some(UncheckedDepositInfo {
                denom: dao_voting::deposit::DepositToken::VotingModuleToken {
                    token_type: VotingModuleTokenType::Cw20,
                },
                amount: Uint128::new(10_000_000),
                refund_policy: DepositRefundPolicy::OnlyPassed,
            }),
            false,
        ),
        close_proposal_on_execution_failure: true,
        query_auth: None,
    }
}

// Same as above but no proposal deposit.
pub(crate) fn get_default_non_token_dao_proposal_module_instantiate(
    app: &mut App,
) -> InstantiateMsg {

    InstantiateMsg {
        veto: None,
        threshold: ThresholdQuorum {
            threshold: PercentageThreshold::Percent(Decimal::percent(15)),
            quorum: PercentageThreshold::Majority {},
        },
        max_voting_period: Duration::Time(604800), // One week.
        min_voting_period: None,
        only_members_execute: true,
        allow_revoting: false,
        pre_propose_info: get_pre_propose_info(app, None, false),
        close_proposal_on_execution_failure: true,
        query_auth: None,
    }
}

pub(crate) fn instantiate_with_native_staked_balances_governance(
    app: &mut App,
    proposal_module_instantiate: InstantiateMsg,
    initial_balances: Option<Vec<InitialBalance>>,
) -> ContractInfo {
    let proposal_module_info = app.store_code(proposal_single_contract());
    let query_auth = app.store_code(query_auth_contract());

    let initial_balances = initial_balances.unwrap_or_else(|| {
        vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(100_000_000),
        }]
    });

    // Collapse balances so that we can test double votes.
    let initial_balances: Vec<InitialBalance> = {
        let mut already_seen = vec![];
        initial_balances
            .into_iter()
            .filter(|InitialBalance { address, amount: _ }| {
                if already_seen.contains(address) {
                    false
                } else {
                    already_seen.push(address.clone());
                    true
                }
            })
            .collect()
    };

    let native_stake_info = app.store_code(native_staked_balances_voting_contract());
    let core_info = app.store_code(cw_core_contract());

    let instantiate_core = dao_interface::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        dao_uri: None,
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: native_stake_info.code_id,
            code_hash: native_stake_info.code_hash.clone(),
            msg: to_binary(&dao_voting_token_staked::msg::InstantiateMsg {
                token_info: dao_voting_token_staked::msg::TokenInfo::Existing {
                    denom: "ujuno".to_string(),
                },
                unstaking_duration: None,
                active_threshold: None,
                query_auth: None,
            })
            .unwrap(),
            admin: None,
            funds: vec![],
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: proposal_module_info.code_id,
            code_hash: proposal_module_info.code_hash.clone(),
            msg: to_binary(&proposal_module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO governance module.".to_string(),
        }],
        initial_items: None,
        query_auth_code_id: query_auth.code_id,
        query_auth_code_hash: query_auth.code_hash,
        prng_seed: "seeed".to_string(),
    };

    let core_contract_info = app
        .instantiate_contract(
            core_info,
            Addr::unchecked(CREATOR_ADDR),
            &instantiate_core,
            &[],
            "DAO DAO",
            None,
        )
        .unwrap();

    let gov_state: dao_interface::query::DumpStateResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &dao_interface::msg::QueryMsg::DumpState {},
        )
        .unwrap();
    let native_staking_addr = gov_state.voting_module;
    let native_staking_code_hash = gov_state.voting_module_code_hash;

    let query_auth_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &dao_interface::msg::QueryMsg::QueryAuthInfo {},
        )
        .unwrap();

    for InitialBalance { address, amount } in initial_balances {
        let viewing_key = create_viewing_key(
            app,
            ContractInfo {
                address: query_auth_info.addr.clone(),
                code_hash: query_auth_info.code_hash.clone(),
            },
            mock_info(&address, &[]),
        );
        app.sudo(SudoMsg::Bank(BankSudo::Mint {
            to_address: address.clone(),
            amount: vec![Coin {
                denom: "ujuno".to_string(),
                amount,
            }],
        }))
        .unwrap();
        app.execute_contract(
            Addr::unchecked(&address),
            &ContractInfo {
                address: native_staking_addr.clone(),
                code_hash: native_staking_code_hash.clone(),
            },
            &dao_voting_token_staked::msg::ExecuteMsg::Stake {
                auth: shade_protocol::basic_staking::Auth::ViewingKey {
                    key: viewing_key,
                    address,
                },
            },
            &[Coin {
                amount,
                denom: "ujuno".to_string(),
            }],
        )
        .unwrap();
    }

    app.update_block(next_block);

    core_contract_info
}

pub(crate) fn instantiate_with_staked_balances_governance(
    app: &mut App,
    proposal_module_instantiate: InstantiateMsg,
    initial_balances: Option<Vec<InitialBalance>>,
) -> ContractInfo {
    let proposal_module_info = app.store_code(proposal_single_contract());
    let query_auth = app.store_code(query_auth_contract());

    let initial_balances = initial_balances.unwrap_or_else(|| {
        vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(100_000_000),
        }]
    });

    // Collapse balances so that we can test double votes.
    let initial_balances: Vec<InitialBalance> = {
        let mut already_seen = vec![];
        initial_balances
            .into_iter()
            .filter(|InitialBalance { address, amount: _ }| {
                if already_seen.contains(address) {
                    false
                } else {
                    already_seen.push(address.clone());
                    true
                }
            })
            .collect()
    };

    let snip20_info = app.store_code(snip20_base_contract());
    let snip20_stake_info = app.store_code(snip20_stake_contract());
    let staked_balances_voting_info = app.store_code(snip20_staked_balances_voting_contract());
    let core_info = app.store_code(cw_core_contract());

    let instantiate_core = dao_interface::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        dao_uri: None,
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: staked_balances_voting_info.code_id,
            code_hash: staked_balances_voting_info.code_hash,
            msg: to_binary(&dao_voting_snip20_staked::msg::InstantiateMsg {
                active_threshold: None,
                token_info: dao_voting_snip20_staked::msg::Snip20TokenInfo::New {
                    code_id: snip20_info.code_id,
                    code_hash: snip20_info.code_hash.clone(),
                    name: "DAO DAO".to_string(),
                    symbol: "DAO".to_string(),
                    decimals: 6,
                    initial_balances: initial_balances.clone(),
                    staking_code_id: snip20_stake_info.code_id,
                    staking_code_hash: snip20_stake_info.code_hash,
                    unstaking_duration: Some(Duration::Height(6)),
                    initial_dao_balance: None,
                },
                query_auth: None,
            })
            .unwrap(),
            admin: None,
            funds: vec![],
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: proposal_module_info.code_id,
            code_hash: proposal_module_info.code_hash,
            msg: to_binary(&proposal_module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO governance module.".to_string(),
        }],
        initial_items: None,
        query_auth_code_id: query_auth.code_id,
        query_auth_code_hash: query_auth.code_hash,
        prng_seed: "seed".to_string(),
    };

    let core_contract_info = app
        .instantiate_contract(
            core_info,
            Addr::unchecked(CREATOR_ADDR),
            &instantiate_core,
            &[],
            "DAO DAO",
            None,
        )
        .unwrap();

    let gov_state: dao_interface::query::DumpStateResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.to_string(),
            &dao_interface::msg::QueryMsg::DumpState {},
        )
        .unwrap();

    let query_auth_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &dao_interface::msg::QueryMsg::QueryAuthInfo {},
        )
        .unwrap();

    let voting_module = gov_state.voting_module;
    let voting_module_code_hash = gov_state.voting_module_code_hash;

    let staking_contract: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_module_code_hash.clone(),
            voting_module.clone(),
            &dao_voting_snip20_staked::msg::QueryMsg::StakingContract {},
        )
        .unwrap();
    let token_contract: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_module_code_hash,
            voting_module,
            &dao_interface::voting::Query::TokenContract {},
        )
        .unwrap();

    // Stake all the initial balances.
    for InitialBalance { address, amount } in initial_balances {
        let viewing_key = create_viewing_key(
            app,
            ContractInfo {
                address: query_auth_info.addr.clone(),
                code_hash: query_auth_info.code_hash.clone(),
            },
            mock_info(&address, &[]),
        );
        app.execute_contract(
            Addr::unchecked(address.clone()),
            &ContractInfo {
                address: token_contract.addr.clone(),
                code_hash: token_contract.code_hash.clone(),
            },
            &snip20_base::msg::ExecuteMsg::Send {
                amount,
                msg: Some(
                    to_binary(&snip20_stake::msg::ReceiveMsg::Stake {
                        auth: Box::new(shade_protocol::basic_staking::Auth::ViewingKey {
                            key: viewing_key,
                            address,
                        }),
                    })
                    .unwrap(),
                ),
                recipient: staking_contract.addr.clone().to_string(),
                recipient_code_hash: Some(staking_contract.code_hash.clone()),
                memo: None,
                decoys: None,
                entropy: None,
                padding: None,
            },
            &[],
        )
        .unwrap();
    }

    // Update the block so that those staked balances appear.
    app.update_block(|block| block.height += 1);

    core_contract_info
}

pub(crate) fn instantiate_with_staking_active_threshold(
    app: &mut App,
    proposal_module_instantiate: InstantiateMsg,
    initial_balances: Option<Vec<InitialBalance>>,
    active_threshold: Option<ActiveThreshold>,
) -> ContractInfo {
    let proposal_module_info = app.store_code(proposal_single_contract());
    let snip20_info = app.store_code(snip20_base_contract());
    let snip20_staking_info = app.store_code(snip20_stake_contract());
    let core_info = app.store_code(cw_core_contract());
    let votemod_info = app.store_code(snip20_staked_balances_voting_contract());
    let query_auth = app.store_code(query_auth_contract());

    let initial_balances = initial_balances.unwrap_or_else(|| {
        vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(100_000_000),
        }]
    });

    let governance_instantiate = dao_interface::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        dao_uri: None,
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: votemod_info.code_id,
            code_hash: votemod_info.code_hash.clone(),
            msg: to_binary(&dao_voting_snip20_staked::msg::InstantiateMsg {
                token_info: dao_voting_snip20_staked::msg::Snip20TokenInfo::New {
                    code_id: snip20_info.code_id,
                    code_hash: snip20_info.code_hash.clone(),
                    name: "DAO".to_string(),
                    symbol: "DAO".to_string(),
                    decimals: 6,
                    initial_balances,
                    staking_code_id: snip20_staking_info.code_id,
                    unstaking_duration: None,
                    initial_dao_balance: None,
                    staking_code_hash: snip20_staking_info.code_hash.clone(),
                },
                active_threshold,
                query_auth: None,
            })
            .unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: proposal_module_info.code_id,
            code_hash: proposal_module_info.code_hash,
            msg: to_binary(&proposal_module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO governance module".to_string(),
        }],
        initial_items: None,
        query_auth_code_id: query_auth.code_id,
        query_auth_code_hash: query_auth.code_hash,
        prng_seed: "seed".into(),
    };

    app.instantiate_contract(
        core_info,
        Addr::unchecked(CREATOR_ADDR),
        &governance_instantiate,
        &[],
        "DAO DAO",
        None,
    )
    .unwrap()
}

pub(crate) fn instantiate_with_cw4_groups_governance(
    app: &mut App,
    proposal_module_instantiate: InstantiateMsg,
    initial_weights: Option<Vec<InitialBalance>>,
) -> ContractInfo {
    let proposal_module_info = app.store_code(proposal_single_contract());
    let cw4_info = app.store_code(cw4_group_contract());
    let core_info = app.store_code(cw_core_contract());
    let votemod_info = app.store_code(cw4_voting_contract());
    let query_auth = app.store_code(query_auth_contract());

    let initial_weights = initial_weights.unwrap_or_else(|| {
        vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(1),
        }]
    });

    // Remove duplicates so that we can test duplicate voting.
    let initial_weights: Vec<cw4::Member> = {
        let mut already_seen = vec![];
        initial_weights
            .into_iter()
            .filter(|InitialBalance { address, .. }| {
                if already_seen.contains(address) {
                    false
                } else {
                    already_seen.push(address.clone());
                    true
                }
            })
            .map(|InitialBalance { address, amount }| cw4::Member {
                addr: address,
                weight: amount.u128() as u64,
            })
            .collect()
    };

    let governance_instantiate = dao_interface::msg::InstantiateMsg {
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        dao_uri: None,
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: votemod_info.code_id,
            code_hash: votemod_info.code_hash,
            msg: to_binary(&dao_voting_cw4::msg::InstantiateMsg {
                group_contract: GroupContract::New {
                    cw4_group_code_id: cw4_info.code_id,
                    initial_members: initial_weights,
                    cw4_group_code_hash: cw4_info.code_hash,
                    query_auth: None,
                },
            })
            .unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: proposal_module_info.code_id,
            msg: to_binary(&proposal_module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO governance module".to_string(),
            code_hash: proposal_module_info.code_hash,
        }],
        initial_items: None,
        query_auth_code_id: query_auth.code_id,
        query_auth_code_hash: query_auth.code_hash,
        prng_seed: "todo!()".to_string(),
    };

    let addr = app
        .instantiate_contract(
            core_info,
            Addr::unchecked(CREATOR_ADDR),
            &governance_instantiate,
            &[],
            "DAO DAO",
            None,
        )
        .unwrap();

    // Update the block so that weights appear.
    app.update_block(|block| block.height += 1);

    addr
}
