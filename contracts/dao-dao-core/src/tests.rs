use cosmwasm_std::{
    from_binary,
    testing::{mock_dependencies, mock_env, mock_info},
    to_binary, Addr, ContractInfo, CosmosMsg, Empty, MessageInfo, Uint128, WasmMsg,
};
use cw4::Member;
use dao_interface::{
    msg::{ExecuteMsg, InitialItem, InstantiateMsg, MigrateMsg, QueryMsg},
    query::{
        AdminNominationResponse, DaoURIResponse, DumpStateResponse, GetItemResponse,
        PauseInfoResponse, ProposalModuleCountResponse, Snip20BalanceResponse, SubDao,
    },
    state::{
        Admin, AnyContractInfo, Config, ModuleInstantiateInfo, ProposalModule,
        ProposalModuleStatus, VotingModuleInfo,
    },
    voting::{InfoResponse, VotingPowerAtHeightResponse},
};
use secret_cw2::ContractVersion;
use secret_multi_test::{
    next_block, App, Contract, ContractInstantiationInfo, ContractWrapper, Executor,
};
use secret_utils::{Duration, Expiration};
use snip20_base::msg::InitialBalance;
use snip721_reference_impl::msg::ReceiverInfo;

use crate::{
    contract::{migrate, CONTRACT_NAME, CONTRACT_VERSION},
    ContractError,
};

const CREATOR_ADDR: &str = "creator";

fn snip20_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip20_base::contract::execute,
        snip20_base::contract::instantiate,
        snip20_base::contract::query,
    );
    Box::new(contract)
}

fn snip721_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip721_reference_impl::contract::execute,
        snip721_reference_impl::contract::instantiate,
        snip721_reference_impl::contract::query,
    );
    Box::new(contract)
}

fn sudo_proposal_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_proposal_sudo::contract::execute,
        dao_proposal_sudo::contract::instantiate,
        dao_proposal_sudo::contract::query,
    );
    Box::new(contract)
}

fn snip20_balances_voting() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_voting_snip20_balance::contract::execute,
        dao_voting_snip20_balance::contract::instantiate,
        dao_voting_snip20_balance::contract::query,
    )
    .with_reply(dao_voting_snip20_balance::contract::reply);
    Box::new(contract)
}

fn cw_core_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_reply(crate::contract::reply)
    .with_migrate(crate::contract::migrate);
    Box::new(contract)
}

fn query_auth_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        query_auth::contract::execute,
        query_auth::contract::instantiate,
        query_auth::contract::query,
    );
    Box::new(contract)
}

fn group_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cw4_group::contract::execute,
        cw4_group::contract::instantiate,
        cw4_group::contract::query,
    );
    Box::new(contract)
}

fn voting_cw4_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_voting_cw4::contract::execute,
        dao_voting_cw4::contract::instantiate,
        dao_voting_cw4::contract::query,
    )
    .with_reply(dao_voting_cw4::contract::reply)
    .with_migrate(dao_voting_cw4::contract::migrate);
    Box::new(contract)
}

fn instantiate_gov(
    app: &mut App,
    contract_instantiation_info: ContractInstantiationInfo,
    msg: InstantiateMsg,
) -> ContractInfo {
    app.instantiate_contract(
        contract_instantiation_info,
        Addr::unchecked(CREATOR_ADDR),
        &msg,
        &[],
        "cw-governance",
        None,
    )
    .unwrap()
}

fn create_token_viewing_key(
    app: &mut App,
    contract_info: ContractInfo,
    info: MessageInfo,
) -> String {
    let msg = snip20_base::msg::ExecuteMsg::CreateViewingKey {
        entropy: "entropy".to_string(),
        padding: None,
    };
    let res = app
        .execute_contract(info.sender, &contract_info, &msg, &[])
        .unwrap();
    let mut viewing_key = String::new();
    let data: snip20_base::msg::ExecuteAnswer = from_binary(&res.data.unwrap()).unwrap();
    if let snip20_base::msg::ExecuteAnswer::CreateViewingKey { key } = data {
        viewing_key = key;
    };
    viewing_key
}

fn test_instantiate_with_gov_modules() -> ContractInfo {
    let mut app = App::default();
    let module_info = app.store_code(voting_cw4_contract());
    let group_contract = app.store_code(group_contract());
    let gov_info = app.store_code(cw_core_contract());
    let query_auth_info = app.store_code(query_auth_contract());
    let module_instantiate = dao_voting_cw4::msg::InstantiateMsg {
        group_contract: dao_voting_cw4::msg::GroupContract::New {
            cw4_group_code_id: group_contract.code_id,
            cw4_group_code_hash: group_contract.code_hash,
            initial_members: vec![Member {
                addr: CREATOR_ADDR.to_string(),
                weight: 1,
            }],
            query_auth: None,
        },
    };
    let instantiate = InstantiateMsg {
        dao_uri: None,
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs.".to_string(),
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: module_info.clone().code_id,
            code_hash: module_info.clone().code_hash,
            msg: to_binary(&module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: module_info.clone().code_id,
            code_hash: module_info.clone().code_hash,
            msg: to_binary(&module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: format!("governance module"),
        }],
        initial_items: None,
        query_auth_code_id: query_auth_info.code_id,
        query_auth_code_hash: query_auth_info.code_hash,
        prng_seed: "seed".to_string(),
    };

    let gov_contract_info = instantiate_gov(&mut app, gov_info, instantiate);
    app.update_block(next_block);

    let state: DumpStateResponse = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash.clone(),
            gov_contract_info.address.clone().to_string(),
            &QueryMsg::DumpState {},
        )
        .unwrap();

    assert_eq!(
        state.config,
        Config {
            dao_uri: None,
            name: "DAO DAO".to_string(),
            description: "A DAO that builds DAOs.".to_string(),
            image_url: None,
        }
    );

    assert_eq!(state.proposal_modules.len(), 1);

    assert_eq!(state.active_proposal_module_count, 1 as u32);

    assert_eq!(state.total_proposal_module_count, 1 as u32);

    gov_contract_info
}

fn test_instantiate_with_0_gov_modules() {
    let mut app = App::default();
    let module_info = app.store_code(voting_cw4_contract());
    let group_contract = app.store_code(group_contract());
    let gov_info = app.store_code(cw_core_contract());
    let query_auth_info = app.store_code(query_auth_contract());
    let module_instantiate = dao_voting_cw4::msg::InstantiateMsg {
        group_contract: dao_voting_cw4::msg::GroupContract::New {
            cw4_group_code_id: group_contract.code_id,
            cw4_group_code_hash: group_contract.code_hash,
            initial_members: vec![Member {
                addr: CREATOR_ADDR.to_string(),
                weight: 1,
            }],
            query_auth: None,
        },
    };
    let instantiate = InstantiateMsg {
        dao_uri: None,
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs.".to_string(),
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: module_info.clone().code_id,
            code_hash: module_info.clone().code_hash,
            msg: to_binary(&module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![],
        initial_items: None,
        query_auth_code_id: query_auth_info.code_id,
        query_auth_code_hash: query_auth_info.code_hash,
        prng_seed: "seed".to_string(),
    };
    let _ = instantiate_gov(&mut app, gov_info, instantiate);
}

#[test]
#[should_panic(expected = "Execution would result in no proposal modules being active.")]
fn test_instantiate_with_zero_gov_modules() {
    test_instantiate_with_0_gov_modules()
}

#[test]
fn test_valid_instantiate() {
    test_instantiate_with_gov_modules();
}

#[test]
fn test_update_config() {
    let mut app = App::default();
    let module_info = app.store_code(voting_cw4_contract());
    let group_contract = app.store_code(group_contract());
    let gov_info = app.store_code(cw_core_contract());
    let query_auth_info = app.store_code(query_auth_contract());
    let module_instantiate = dao_voting_cw4::msg::InstantiateMsg {
        group_contract: dao_voting_cw4::msg::GroupContract::New {
            cw4_group_code_id: group_contract.code_id,
            cw4_group_code_hash: group_contract.code_hash,
            initial_members: vec![Member {
                addr: CREATOR_ADDR.to_string(),
                weight: 1,
            }],
            query_auth: None,
        },
    };
    let instantiate = InstantiateMsg {
        dao_uri: None,
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs.".to_string(),
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: module_info.clone().code_id,
            code_hash: module_info.clone().code_hash,
            msg: to_binary(&module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: module_info.clone().code_id,
            code_hash: module_info.clone().code_hash,
            msg: to_binary(&module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: format!("governance module"),
        }],
        initial_items: None,
        query_auth_code_id: query_auth_info.code_id,
        query_auth_code_hash: query_auth_info.code_hash,
        prng_seed: "seed".to_string(),
    };

    let gov_contract_info = instantiate_gov(&mut app, gov_info, instantiate);
    let modules: Vec<ProposalModule> = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash.clone(),
            gov_contract_info.address.clone(),
            &QueryMsg::ProposalModules {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(modules.len(), 1);

    let expected_config = Config {
        dao_uri: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs.".to_string(),
        image_url: None,
    };

    let config: Config = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash.clone(),
            gov_contract_info.address.clone(),
            &QueryMsg::Config {},
        )
        .unwrap();

    assert_eq!(expected_config, config);

    let dao_uri: DaoURIResponse = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash,
            gov_contract_info.address,
            &QueryMsg::DaoURI {},
        )
        .unwrap();
    assert_eq!(dao_uri.dao_uri, expected_config.dao_uri);
}

fn test_swap_governance(swaps: Vec<(u32, u32)>) {
    let mut app = App::default();
    let module_info = app.store_code(voting_cw4_contract());
    let group_contract = app.store_code(group_contract());
    let gov_info = app.store_code(cw_core_contract());
    let query_auth_info = app.store_code(query_auth_contract());
    let module_instantiate = dao_voting_cw4::msg::InstantiateMsg {
        group_contract: dao_voting_cw4::msg::GroupContract::New {
            cw4_group_code_id: group_contract.code_id,
            cw4_group_code_hash: group_contract.code_hash,
            initial_members: vec![Member {
                addr: CREATOR_ADDR.to_string(),
                weight: 1,
            }],
            query_auth: None,
        },
    };
    let instantiate = InstantiateMsg {
        dao_uri: None,
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs.".to_string(),
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: module_info.clone().code_id,
            code_hash: module_info.clone().code_hash,
            msg: to_binary(&module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: module_info.clone().code_id,
            code_hash: module_info.clone().code_hash,
            msg: to_binary(&module_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: format!("governance module"),
        }],
        initial_items: None,
        query_auth_code_id: query_auth_info.code_id,
        query_auth_code_hash: query_auth_info.code_hash,
        prng_seed: "seed".to_string(),
    };

    let gov_contract_info = instantiate_gov(&mut app, gov_info, instantiate);
    let modules: Vec<ProposalModule> = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash.clone(),
            gov_contract_info.address.clone(),
            &QueryMsg::ProposalModules {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(modules.len(), 1);

    let module_count = query_proposal_module_count(&app, &gov_contract_info.clone());
    assert_eq!(
        module_count,
        ProposalModuleCountResponse {
            active_proposal_module_count: 1,
            total_proposal_module_count: 1,
        }
    );

    let (to_add, to_remove) = swaps
        .iter()
        .cloned()
        .reduce(|(to_add, to_remove), (add, remove)| (to_add + add, to_remove + remove))
        .unwrap_or((0, 0));

    for (add, remove) in swaps {
        let start_modules: Vec<ProposalModule> = app
            .wrap()
            .query_wasm_smart(
                gov_contract_info.code_hash.clone(),
                gov_contract_info.address.clone(),
                &QueryMsg::ProposalModules {
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();

        let start_modules_active: Vec<ProposalModule> =
            get_active_modules(&app, gov_contract_info.clone());

        let to_add: Vec<_> = (0..add)
            .map(|n| ModuleInstantiateInfo {
                code_id: module_info.code_id,
                code_hash: module_info.code_hash.clone(),
                msg: to_binary(&module_instantiate).unwrap(),
                admin: Some(Admin::CoreModule {}),
                funds: vec![],
                label: format!("governance module {n}"),
            })
            .collect();

        let to_disable: Vec<_> = start_modules_active
            .iter()
            .rev()
            .take(remove as usize)
            .map(|a| a.address.to_string())
            .collect();
        println!("{:?}", to_disable);

        app.execute_contract(
            Addr::unchecked(gov_contract_info.address.clone().into_string()),
            &ContractInfo {
                address: gov_contract_info.address.clone(),
                code_hash: gov_contract_info.code_hash.clone(),
            },
            &ExecuteMsg::UpdateProposalModules { to_add, to_disable },
            &[],
        )
        .unwrap();
        app.update_block(next_block);

        let finish_modules_active = get_active_modules(&app, gov_contract_info.clone());

        for module in start_modules
            .clone()
            .into_iter()
            .rev()
            .take(remove as usize)
        {
            assert!(!finish_modules_active.contains(&module))
        }
    }

    let module_count = query_proposal_module_count(&app, &gov_contract_info);
    println!("{:?}", module_count);
    assert_eq!(
        module_count,
        ProposalModuleCountResponse {
            active_proposal_module_count: 1 + to_add - to_remove,
            total_proposal_module_count: 1 + to_add,
        }
    );
}

#[test]
fn test_update_governance() {
    test_swap_governance(vec![(1, 1)])
}

#[test]
fn test_add_then_remove_governance() {
    test_swap_governance(vec![(1, 0), (0, 1)])
}

#[test]
fn test_removed_modules_can_not_execute() {
    let mut app = App::default();
    let govmod_info = app.store_code(sudo_proposal_contract());
    let gov_info = app.store_code(cw_core_contract());
    let query_auth_info = app.store_code(query_auth_contract());

    let govmod_instantiate = dao_proposal_sudo::msg::InstantiateMsg {
        root: CREATOR_ADDR.to_string(),
        dao_code_hash: gov_info.code_hash.clone(),
    };
    let gov_instantiate = InstantiateMsg {
        dao_uri: None,
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs.".to_string(),
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: govmod_info.code_id.clone(),
            code_hash: govmod_info.code_hash.clone(),
            msg: to_binary(&govmod_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: govmod_info.code_id.clone(),
            code_hash: govmod_info.code_hash.clone(),
            msg: to_binary(&govmod_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "governance module".to_string(),
        }],
        initial_items: None,
        query_auth_code_id: query_auth_info.code_id,
        query_auth_code_hash: query_auth_info.code_hash,
        prng_seed: "seed".to_string(),
    };

    let gov_contract_info = app
        .instantiate_contract(
            gov_info,
            Addr::unchecked(CREATOR_ADDR),
            &gov_instantiate,
            &[],
            "cw-governance",
            None,
        )
        .unwrap();

    let modules: Vec<ProposalModule> = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash.clone(),
            gov_contract_info.address.clone(),
            &QueryMsg::ProposalModules {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(modules.len(), 1);

    let start_module = modules.into_iter().next().unwrap();

    let to_add = vec![ModuleInstantiateInfo {
        code_id: govmod_info.code_id.clone(),
        code_hash: govmod_info.code_hash.clone(),
        msg: to_binary(&govmod_instantiate).unwrap(),
        admin: Some(Admin::CoreModule {}),
        funds: vec![],
        label: "new governance module".to_string(),
    }];

    let to_disable = vec![start_module.address.to_string()];

    // Swap ourselves out.
    app.execute_contract(
        Addr::unchecked(CREATOR_ADDR),
        &ContractInfo {
            address: start_module.address.clone(),
            code_hash: start_module.code_hash.clone(),
        },
        &dao_proposal_sudo::msg::ExecuteMsg::Execute {
            msgs: vec![WasmMsg::Execute {
                contract_addr: gov_contract_info.address.clone().to_string(),
                code_hash: gov_contract_info.code_hash.clone(),
                funds: vec![],
                msg: to_binary(&ExecuteMsg::UpdateProposalModules { to_add, to_disable }).unwrap(),
            }
            .into()],
        },
        &[],
    )
    .unwrap();

    app.update_block(next_block);

    let finish_modules_active: Vec<ProposalModule> =
        get_active_modules(&app, gov_contract_info.clone());
    println!("{:?}", finish_modules_active);

    let new_proposal_module = finish_modules_active.into_iter().next().unwrap();

    // Try to add a new module and remove the one we added
    // earlier. This should fail as we have been removed.
    let to_add = vec![ModuleInstantiateInfo {
        code_id: govmod_info.code_id.clone(),
        code_hash: govmod_info.code_hash.clone(),
        msg: to_binary(&govmod_instantiate).unwrap(),
        admin: Some(Admin::CoreModule {}),
        funds: vec![],
        label: "new governance module".to_string(),
    }];
    let to_disable = vec![new_proposal_module.address.to_string()];

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(CREATOR_ADDR),
            &ContractInfo {
                address: start_module.address.clone(),
                code_hash: start_module.code_hash.clone(),
            },
            &dao_proposal_sudo::msg::ExecuteMsg::Execute {
                msgs: vec![WasmMsg::Execute {
                    contract_addr: gov_contract_info.address.clone().to_string(),
                    code_hash: gov_contract_info.code_hash.clone(),
                    funds: vec![],
                    msg: to_binary(&ExecuteMsg::UpdateProposalModules {
                        to_add: to_add.clone(),
                        to_disable: to_disable.clone(),
                    })
                    .unwrap(),
                }
                .into()],
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert!(matches!(
        err,
        ContractError::ModuleDisabledCannotExecute {
            address: _gov_address
        }
    ));

    // Check that the enabled query works.
    let enabled_modules: Vec<ProposalModule> = app
        .wrap()
        .query_wasm_smart(
            &gov_contract_info.code_hash.clone(),
            gov_contract_info.address.clone(),
            &QueryMsg::ActiveProposalModules {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(enabled_modules, vec![new_proposal_module.clone()]);

    // The new proposal module should be able to perform actions.
    app.execute_contract(
        Addr::unchecked(CREATOR_ADDR),
        &ContractInfo {
            address: new_proposal_module.address.clone(),
            code_hash: new_proposal_module.code_hash.clone(),
        },
        &dao_proposal_sudo::msg::ExecuteMsg::Execute {
            msgs: vec![WasmMsg::Execute {
                contract_addr: gov_contract_info.address.to_string(),
                code_hash: gov_contract_info.code_hash.to_string(),
                funds: vec![],
                msg: to_binary(&ExecuteMsg::UpdateProposalModules { to_add, to_disable }).unwrap(),
            }
            .into()],
        },
        &[],
    )
    .unwrap();
}

#[test]
fn test_module_already_disabled() {
    let mut app = App::default();
    let govmod_info = app.store_code(sudo_proposal_contract());
    let gov_info = app.store_code(cw_core_contract());
    let query_auth_info = app.store_code(query_auth_contract());

    let govmod_instantiate = dao_proposal_sudo::msg::InstantiateMsg {
        root: CREATOR_ADDR.to_string(),
        dao_code_hash: gov_info.code_hash.clone(),
    };
    let gov_instantiate = InstantiateMsg {
        dao_uri: None,
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs.".to_string(),
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: govmod_info.code_id.clone(),
            code_hash: govmod_info.code_hash.clone(),
            msg: to_binary(&govmod_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: govmod_info.code_id.clone(),
            code_hash: govmod_info.code_hash.clone(),
            msg: to_binary(&govmod_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "governance module".to_string(),
        }],
        initial_items: None,
        query_auth_code_id: query_auth_info.code_id,
        query_auth_code_hash: query_auth_info.code_hash,
        prng_seed: "seed".to_string(),
    };

    let gov_contract_info = app
        .instantiate_contract(
            gov_info,
            Addr::unchecked(CREATOR_ADDR),
            &gov_instantiate,
            &[],
            "cw-governance",
            None,
        )
        .unwrap();

    let modules: Vec<ProposalModule> = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash.clone(),
            gov_contract_info.address.clone(),
            &QueryMsg::ProposalModules {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(modules.len(), 1);

    let start_module = modules.into_iter().next().unwrap();

    let to_disable = vec![
        start_module.address.to_string(),
        start_module.address.to_string(),
    ];

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(CREATOR_ADDR),
            &ContractInfo {
                address: start_module.address.clone(),
                code_hash: start_module.code_hash.clone(),
            },
            &dao_proposal_sudo::msg::ExecuteMsg::Execute {
                msgs: vec![WasmMsg::Execute {
                    contract_addr: gov_contract_info.address.clone().to_string(),
                    code_hash: gov_contract_info.code_hash.clone(),
                    funds: vec![],
                    msg: to_binary(&ExecuteMsg::UpdateProposalModules {
                        to_add: vec![ModuleInstantiateInfo {
                            code_id: govmod_info.code_id.clone(),
                            code_hash: govmod_info.code_hash.clone(),
                            msg: to_binary(&govmod_instantiate).unwrap(),
                            admin: Some(Admin::CoreModule {}),
                            funds: vec![],
                            label: "governance module".to_string(),
                        }],
                        to_disable,
                    })
                    .unwrap(),
                }
                .into()],
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    assert_eq!(
        err,
        ContractError::ModuleAlreadyDisabled {
            address: start_module.address
        }
    )
}

#[test]
fn test_swap_voting_module() {
    let mut app = App::default();
    let govmod_info = app.store_code(sudo_proposal_contract());
    let gov_info = app.store_code(cw_core_contract());
    let query_auth_info = app.store_code(query_auth_contract());

    let govmod_instantiate = dao_proposal_sudo::msg::InstantiateMsg {
        root: CREATOR_ADDR.to_string(),
        dao_code_hash: gov_info.code_hash.clone(),
    };
    let gov_instantiate = InstantiateMsg {
        dao_uri: None,
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs.".to_string(),
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: govmod_info.code_id.clone(),
            code_hash: govmod_info.code_hash.clone(),
            msg: to_binary(&govmod_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: govmod_info.code_id.clone(),
            code_hash: govmod_info.code_hash.clone(),
            msg: to_binary(&govmod_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "governance module".to_string(),
        }],
        initial_items: None,
        query_auth_code_id: query_auth_info.code_id,
        query_auth_code_hash: query_auth_info.code_hash,
        prng_seed: "seed".to_string(),
    };

    let gov_contract_info = app
        .instantiate_contract(
            gov_info,
            Addr::unchecked(CREATOR_ADDR),
            &gov_instantiate,
            &[],
            "cw-governance",
            None,
        )
        .unwrap();

    let voting_module: VotingModuleInfo = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash.clone(),
            gov_contract_info.address.clone(),
            &QueryMsg::VotingModule {},
        )
        .unwrap();

    let modules: Vec<ProposalModule> = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash.clone(),
            gov_contract_info.address.clone(),
            &QueryMsg::ProposalModules {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(modules.len(), 1);

    app.execute_contract(
        Addr::unchecked(CREATOR_ADDR),
        &ContractInfo {
            address: modules[0].address.clone(),
            code_hash: modules[0].code_hash.clone(),
        },
        &dao_proposal_sudo::msg::ExecuteMsg::Execute {
            msgs: vec![WasmMsg::Execute {
                contract_addr: gov_contract_info.address.clone().to_string(),
                code_hash: gov_contract_info.code_hash.clone(),
                funds: vec![],
                msg: to_binary(&ExecuteMsg::UpdateVotingModule {
                    module: ModuleInstantiateInfo {
                        code_id: govmod_info.code_id.clone(),
                        code_hash: govmod_info.code_hash.clone(),
                        msg: to_binary(&govmod_instantiate).unwrap(),
                        admin: Some(Admin::CoreModule {}),
                        funds: vec![],
                        label: "voting module".to_string(),
                    },
                })
                .unwrap(),
            }
            .into()],
        },
        &[],
    )
    .unwrap();

    let new_voting_module: VotingModuleInfo = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash,
            gov_contract_info.address,
            &QueryMsg::VotingModule {},
        )
        .unwrap();

    assert_ne!(new_voting_module, voting_module);
}

fn test_unauthorized(app: &mut App, gov_contract_info: ContractInfo, msg: ExecuteMsg) {
    let err: ContractError = app
        .execute_contract(Addr::unchecked(CREATOR_ADDR), &gov_contract_info, &msg, &[])
        .unwrap_err()
        .downcast()
        .unwrap();

    assert_eq!(err, ContractError::Unauthorized {});
}

#[test]
fn test_permissions() {
    let mut app = App::default();
    let govmod_info = app.store_code(sudo_proposal_contract());
    let gov_info = app.store_code(cw_core_contract());
    let query_auth_info = app.store_code(query_auth_contract());

    let govmod_instantiate = dao_proposal_sudo::msg::InstantiateMsg {
        root: CREATOR_ADDR.to_string(),
        dao_code_hash: gov_info.code_hash.clone(),
    };
    let gov_instantiate = InstantiateMsg {
        dao_uri: None,
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs.".to_string(),
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: govmod_info.code_id.clone(),
            code_hash: govmod_info.code_hash.clone(),
            msg: to_binary(&govmod_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: govmod_info.code_id.clone(),
            code_hash: govmod_info.code_hash.clone(),
            msg: to_binary(&govmod_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "governance module".to_string(),
        }],
        initial_items: None,
        query_auth_code_id: query_auth_info.code_id,
        query_auth_code_hash: query_auth_info.code_hash,
        prng_seed: "seed".to_string(),
    };

    let gov_contract_info = app
        .instantiate_contract(
            gov_info,
            Addr::unchecked(CREATOR_ADDR),
            &gov_instantiate,
            &[],
            "cw-governance",
            None,
        )
        .unwrap();

    test_unauthorized(
        &mut app,
        gov_contract_info.clone(),
        ExecuteMsg::UpdateVotingModule {
            module: ModuleInstantiateInfo {
                code_id: govmod_info.code_id.clone(),
                code_hash: govmod_info.code_hash.clone(),
                msg: to_binary(&govmod_instantiate).unwrap(),
                admin: Some(Admin::CoreModule {}),
                funds: vec![],
                label: "voting module".to_string(),
            },
        },
    );

    test_unauthorized(
        &mut app,
        gov_contract_info.clone(),
        ExecuteMsg::UpdateProposalModules {
            to_add: vec![],
            to_disable: vec![],
        },
    );

    test_unauthorized(
        &mut app,
        gov_contract_info,
        ExecuteMsg::UpdateConfig {
            config: Config {
                dao_uri: None,
                name: "Evil config.".to_string(),
                description: "👿".to_string(),
                image_url: None,
            },
        },
    );
}

fn do_standard_instantiate(_auto_add: bool, admin: Option<String>) -> (ContractInfo, App) {
    let mut app = App::default();
    let govmod_info = app.store_code(sudo_proposal_contract());
    let voting_info = app.store_code(snip20_balances_voting());
    let gov_info = app.store_code(cw_core_contract());
    let snip20_info = app.store_code(snip20_contract());
    let query_auth_info = app.store_code(query_auth_contract());

    let govmod_instantiate = dao_proposal_sudo::msg::InstantiateMsg {
        root: CREATOR_ADDR.to_string(),
        dao_code_hash: gov_info.code_hash.clone(),
    };
    let voting_instantiate = dao_voting_snip20_balance::msg::InstantiateMsg {
        token_info: dao_voting_snip20_balance::msg::TokenInfo::New {
            code_id: snip20_info.code_id.clone(),
            code_hash: snip20_info.code_hash.clone(),
            label: "DAO DAO voting".to_string(),
            name: "DAO DAO".to_string(),
            symbol: "DAO".to_string(),
            decimals: 6,
            initial_balances: vec![InitialBalance {
                address: CREATOR_ADDR.to_string(),
                amount: Uint128::new(2),
            }],
        },
        dao_code_hash: gov_info.code_hash.clone(),
    };

    let gov_instantiate = InstantiateMsg {
        dao_uri: None,
        admin,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs.".to_string(),
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: voting_info.code_id.clone(),
            code_hash: voting_info.code_hash.clone(),
            msg: to_binary(&voting_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: govmod_info.code_id.clone(),
            code_hash: govmod_info.code_hash.clone(),
            msg: to_binary(&govmod_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "governance module".to_string(),
        }],
        initial_items: None,
        query_auth_code_id: query_auth_info.code_id,
        query_auth_code_hash: query_auth_info.code_hash,
        prng_seed: "seed".to_string(),
    };

    let gov_contract_info = app
        .instantiate_contract(
            gov_info,
            Addr::unchecked(CREATOR_ADDR),
            &gov_instantiate,
            &[],
            "cw-governance",
            None,
        )
        .unwrap();

    (gov_contract_info, app)
}

#[test]
fn test_admin_permissions() {
    let (core_contract_info, mut app) = do_standard_instantiate(true, None);

    let start_height = app.block_info().height;
    let proposal_modules: Vec<ProposalModule> = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::ProposalModules {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(proposal_modules.len(), 1);
    let proposal_module = proposal_modules.into_iter().next().unwrap();

    // Random address can't call ExecuteAdminMsgs
    let res = app.execute_contract(
        Addr::unchecked("random"),
        &core_contract_info.clone(),
        &ExecuteMsg::ExecuteAdminMsgs {
            msgs: vec![WasmMsg::Execute {
                contract_addr: core_contract_info.address.clone().to_string(),
                code_hash: core_contract_info.code_hash.clone(),
                msg: to_binary(&ExecuteMsg::Pause {
                    duration: Duration::Height(10),
                })
                .unwrap(),
                funds: vec![],
            }
            .into()],
        },
        &[],
    );
    res.unwrap_err();

    // Proposal mdoule can't call ExecuteAdminMsgs
    let res = app.execute_contract(
        proposal_module.address.clone(),
        &core_contract_info.clone(),
        &ExecuteMsg::ExecuteAdminMsgs {
            msgs: vec![WasmMsg::Execute {
                contract_addr: core_contract_info.address.clone().to_string(),
                code_hash: core_contract_info.code_hash.clone(),
                msg: to_binary(&ExecuteMsg::Pause {
                    duration: Duration::Height(10),
                })
                .unwrap(),
                funds: vec![],
            }
            .into()],
        },
        &[],
    );
    res.unwrap_err();

    // Update Admin can't be called by non-admins
    let res = app.execute_contract(
        Addr::unchecked("rando"),
        &core_contract_info.clone(),
        &ExecuteMsg::NominateAdmin {
            admin: Some("rando".to_string()),
        },
        &[],
    );
    res.unwrap_err();

    // Nominate admin can be called by core contract as no admin was
    // specified so the admin defaulted to the core contract.
    let res = app.execute_contract(
        proposal_module.address.clone(),
        &core_contract_info.clone(),
        &ExecuteMsg::ExecuteProposalHook {
            msgs: vec![WasmMsg::Execute {
                contract_addr: core_contract_info.address.clone().to_string(),
                code_hash: core_contract_info.code_hash.clone(),
                msg: to_binary(&ExecuteMsg::NominateAdmin {
                    admin: Some("meow".to_string()),
                })
                .unwrap(),
                funds: vec![],
            }
            .into()],
        },
        &[],
    );
    res.unwrap();

    // Instantiate new DAO with an admin
    let (core_with_admin_addr, mut app) =
        do_standard_instantiate(true, Some(Addr::unchecked("admin").to_string()));

    // Non admins still can't call ExecuteAdminMsgs
    let res = app.execute_contract(
        proposal_module.address,
        &core_with_admin_addr.clone(),
        &ExecuteMsg::ExecuteAdminMsgs {
            msgs: vec![WasmMsg::Execute {
                contract_addr: core_with_admin_addr.address.clone().to_string(),
                code_hash: core_with_admin_addr.code_hash.clone(),
                msg: to_binary(&ExecuteMsg::Pause {
                    duration: Duration::Height(10),
                })
                .unwrap(),
                funds: vec![],
            }
            .into()],
        },
        &[],
    );
    res.unwrap_err();

    // Admin can call ExecuteAdminMsgs, here an admin pasues the DAO
    let res = app.execute_contract(
        Addr::unchecked("admin"),
        &core_with_admin_addr.clone(),
        &ExecuteMsg::ExecuteAdminMsgs {
            msgs: vec![WasmMsg::Execute {
                contract_addr: core_with_admin_addr.address.clone().to_string(),
                code_hash: core_with_admin_addr.code_hash.clone(),
                msg: to_binary(&ExecuteMsg::Pause {
                    duration: Duration::Height(10),
                })
                .unwrap(),
                funds: vec![],
            }
            .into()],
        },
        &[],
    );
    res.unwrap();

    let paused: PauseInfoResponse = app
        .wrap()
        .query_wasm_smart(
            core_with_admin_addr.code_hash.clone(),
            core_with_admin_addr.address.clone(),
            &QueryMsg::PauseInfo {},
        )
        .unwrap();
    assert_eq!(
        paused,
        PauseInfoResponse::Paused {
            expiration: Expiration::AtHeight(start_height + 10)
        }
    );

    // DAO unpauses after 10 blocks
    app.update_block(|block| block.height += 11);

    // Admin can nominate a new admin.
    let res = app.execute_contract(
        Addr::unchecked("admin"),
        &core_with_admin_addr.clone(),
        &ExecuteMsg::NominateAdmin {
            admin: Some("meow".to_string()),
        },
        &[],
    );
    res.unwrap();

    let nomination: AdminNominationResponse = app
        .wrap()
        .query_wasm_smart(
            core_with_admin_addr.code_hash.clone(),
            core_with_admin_addr.address.clone(),
            &QueryMsg::AdminNomination {},
        )
        .unwrap();
    assert_eq!(
        nomination,
        AdminNominationResponse {
            nomination: Some(Addr::unchecked("meow"))
        }
    );

    // Check that admin has not yet been updated
    let res: Addr = app
        .wrap()
        .query_wasm_smart(
            core_with_admin_addr.code_hash.clone(),
            core_with_admin_addr.address.clone(),
            &QueryMsg::Admin {},
        )
        .unwrap();
    assert_eq!(res, Addr::unchecked("admin"));

    // Only the nominated address may accept the nomination.
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("random"),
            &core_with_admin_addr.clone(),
            &ExecuteMsg::AcceptAdminNomination {},
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::Unauthorized {});

    // Accept the nomination.
    app.execute_contract(
        Addr::unchecked("meow"),
        &core_with_admin_addr.clone(),
        &ExecuteMsg::AcceptAdminNomination {},
        &[],
    )
    .unwrap();

    // Check that admin has been updated
    let res: Addr = app
        .wrap()
        .query_wasm_smart(
            core_with_admin_addr.code_hash.clone(),
            core_with_admin_addr.address.clone(),
            &QueryMsg::Admin {},
        )
        .unwrap();
    assert_eq!(res, Addr::unchecked("meow"));

    // Check that the pending admin has been cleared.
    let nomination: AdminNominationResponse = app
        .wrap()
        .query_wasm_smart(
            core_with_admin_addr.code_hash,
            core_with_admin_addr.address,
            &QueryMsg::AdminNomination {},
        )
        .unwrap();
    assert_eq!(nomination, AdminNominationResponse { nomination: None });
}

#[test]
fn test_admin_nomination() {
    let (core_contract_info, mut app) = do_standard_instantiate(true, Some("admin".to_string()));

    // Check that there is no pending nominations.
    let nomination: AdminNominationResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::AdminNomination {},
        )
        .unwrap();
    assert_eq!(nomination, AdminNominationResponse { nomination: None });

    // Nominate a new admin.
    app.execute_contract(
        Addr::unchecked("admin"),
        &core_contract_info.clone(),
        &ExecuteMsg::NominateAdmin {
            admin: Some("ekez".to_string()),
        },
        &[],
    )
    .unwrap();

    // Check that the nomination is in place.
    let nomination: AdminNominationResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::AdminNomination {},
        )
        .unwrap();
    assert_eq!(
        nomination,
        AdminNominationResponse {
            nomination: Some(Addr::unchecked("ekez"))
        }
    );

    // Non-admin can not withdraw.
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("ekez"),
            &core_contract_info.clone(),
            &ExecuteMsg::WithdrawAdminNomination {},
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::Unauthorized {});

    // Admin can withdraw.
    app.execute_contract(
        Addr::unchecked("admin"),
        &core_contract_info.clone(),
        &ExecuteMsg::WithdrawAdminNomination {},
        &[],
    )
    .unwrap();

    // Check that the nomination is withdrawn.
    let nomination: AdminNominationResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::AdminNomination {},
        )
        .unwrap();
    assert_eq!(nomination, AdminNominationResponse { nomination: None });

    // Can not withdraw if no nomination is pending.
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("admin"),
            &core_contract_info.clone(),
            &ExecuteMsg::WithdrawAdminNomination {},
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::NoAdminNomination {});

    // Can not claim nomination b/c it has been withdrawn.
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("ekez"),
            &core_contract_info.clone(),
            &ExecuteMsg::AcceptAdminNomination {},
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::NoAdminNomination {});

    // Nominate a new admin.
    app.execute_contract(
        Addr::unchecked("admin"),
        &core_contract_info.clone(),
        &ExecuteMsg::NominateAdmin {
            admin: Some("meow".to_string()),
        },
        &[],
    )
    .unwrap();

    // A new nomination can not be created if there is already a
    // pending nomination.
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("admin"),
            &core_contract_info.clone(),
            &ExecuteMsg::NominateAdmin {
                admin: Some("arthur".to_string()),
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::PendingNomination {});

    // Only nominated admin may accept.
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("ekez"),
            &core_contract_info.clone(),
            &ExecuteMsg::AcceptAdminNomination {},
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::Unauthorized {});

    app.execute_contract(
        Addr::unchecked("meow"),
        &core_contract_info.clone(),
        &ExecuteMsg::AcceptAdminNomination {},
        &[],
    )
    .unwrap();

    // Check that meow is the new admin.
    let admin: Addr = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::Admin {},
        )
        .unwrap();
    assert_eq!(admin, Addr::unchecked("meow".to_string()));

    let start_height = app.block_info().height;
    // Check that the new admin can do admin things and the old can not.
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("admin"),
            &core_contract_info.clone(),
            &ExecuteMsg::ExecuteAdminMsgs {
                msgs: vec![WasmMsg::Execute {
                    contract_addr: core_contract_info.address.clone().to_string(),
                    code_hash: core_contract_info.code_hash.clone(),
                    msg: to_binary(&ExecuteMsg::Pause {
                        duration: Duration::Height(10),
                    })
                    .unwrap(),
                    funds: vec![],
                }
                .into()],
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::Unauthorized {});

    let res = app.execute_contract(
        Addr::unchecked("meow"),
        &core_contract_info.clone(),
        &ExecuteMsg::ExecuteAdminMsgs {
            msgs: vec![WasmMsg::Execute {
                contract_addr: core_contract_info.address.clone().to_string(),
                code_hash: core_contract_info.code_hash.clone(),
                msg: to_binary(&ExecuteMsg::Pause {
                    duration: Duration::Height(10),
                })
                .unwrap(),
                funds: vec![],
            }
            .into()],
        },
        &[],
    );
    res.unwrap();

    let paused: PauseInfoResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::PauseInfo {},
        )
        .unwrap();
    assert_eq!(
        paused,
        PauseInfoResponse::Paused {
            expiration: Expiration::AtHeight(start_height + 10)
        }
    );

    // DAO unpauses after 10 blocks
    app.update_block(|block| block.height += 11);

    // Remove the admin.
    app.execute_contract(
        Addr::unchecked("meow"),
        &core_contract_info.clone(),
        &ExecuteMsg::NominateAdmin { admin: None },
        &[],
    )
    .unwrap();

    // Check that this has not caused an admin to be nominated.
    let nomination: AdminNominationResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::AdminNomination {},
        )
        .unwrap();
    assert_eq!(nomination, AdminNominationResponse { nomination: None });

    // Check that admin has been updated. As there was no admin
    // nominated the admin should revert back to the contract address.
    let res: Addr = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::Admin {},
        )
        .unwrap();
    assert_eq!(res, core_contract_info.address);
}

#[test]
fn test_passthrough_voting_queries() {
    let (gov_conract_info, mut app) = do_standard_instantiate(true, None);

    let voting_module: VotingModuleInfo = app
        .wrap()
        .query_wasm_smart(
            gov_conract_info.code_hash.clone(),
            gov_conract_info.address.clone(),
            &QueryMsg::VotingModule {},
        )
        .unwrap();

    let token_contract: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_module.code_hash.clone(),
            voting_module.addr.clone(),
            &dao_voting_snip20_balance::msg::QueryMsg::TokenContract {},
        )
        .unwrap();

    let viewing_key_token = create_token_viewing_key(
        &mut app,
        ContractInfo {
            address: token_contract.addr,
            code_hash: token_contract.code_hash,
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            gov_conract_info.code_hash.clone(),
            gov_conract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: shade_protocol::basic_staking::Auth::ViewingKey {
                    key: viewing_key_token.clone(),
                    address: CREATOR_ADDR.to_string(),
                },
                height: None,
            },
        )
        .unwrap();

    assert_eq!(
        creator_voting_power,
        VotingPowerAtHeightResponse {
            power: Uint128::from(2u64),
            height: app.block_info().height,
        }
    );
}

fn set_item(app: &mut App, gov_contract_info: ContractInfo, key: String, value: String) {
    app.execute_contract(
        gov_contract_info.address.clone(),
        &gov_contract_info,
        &ExecuteMsg::SetItem { key, value },
        &[],
    )
    .unwrap();
}

fn remove_item(app: &mut App, gov_contract_info: ContractInfo, key: String) {
    app.execute_contract(
        gov_contract_info.address.clone(),
        &gov_contract_info,
        &ExecuteMsg::RemoveItem { key },
        &[],
    )
    .unwrap();
}

fn get_item(app: &mut App, gov_contract_info: ContractInfo, key: String) -> GetItemResponse {
    app.wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash,
            gov_contract_info.address,
            &QueryMsg::GetItem { key },
        )
        .unwrap()
}

fn list_items(
    app: &mut App,
    gov_contract_info: ContractInfo,
    start_at: Option<String>,
    limit: Option<u32>,
) -> Vec<(String, String)> {
    app.wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash,
            gov_contract_info.address,
            &QueryMsg::ListItems {
                start_after: start_at,
                limit,
            },
        )
        .unwrap()
}

#[test]
fn test_item_permissions() {
    let (gov_contract_info, mut app) = do_standard_instantiate(true, None);

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("ekez"),
            &gov_contract_info.clone(),
            &ExecuteMsg::SetItem {
                key: "k".to_string(),
                value: "v".to_string(),
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::Unauthorized {});

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("ekez"),
            &gov_contract_info,
            &ExecuteMsg::RemoveItem {
                key: "k".to_string(),
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::Unauthorized {});
}

#[test]
fn test_add_remove_get() {
    let (gov_contract_info, mut app) = do_standard_instantiate(true, None);

    let a = get_item(&mut app, gov_contract_info.clone(), "aaaaa".to_string());
    assert_eq!(a, GetItemResponse { item: None });

    set_item(
        &mut app,
        gov_contract_info.clone(),
        "aaaaakey".to_string(),
        "aaaaaaddr".to_string(),
    );
    let a = get_item(&mut app, gov_contract_info.clone(), "aaaaakey".to_string());
    assert_eq!(
        a,
        GetItemResponse {
            item: Some("aaaaaaddr".to_string())
        }
    );

    remove_item(&mut app, gov_contract_info.clone(), "aaaaakey".to_string());
    let a = get_item(&mut app, gov_contract_info, "aaaaakey".to_string());
    assert_eq!(a, GetItemResponse { item: None });
}

#[test]
#[should_panic(expected = "Key is missing from storage")]
fn test_remove_missing_key() {
    let (gov_contract_info, mut app) = do_standard_instantiate(true, None);
    remove_item(&mut app, gov_contract_info, "b".to_string())
}

#[test]
fn test_list_items() {
    let mut app = App::default();
    let govmod_info = app.store_code(sudo_proposal_contract());
    let voting_info = app.store_code(snip20_balances_voting());
    let gov_info = app.store_code(cw_core_contract());
    let snip20_info = app.store_code(snip20_contract());
    let query_auth_info = app.store_code(query_auth_contract());

    let govmod_instantiate = dao_proposal_sudo::msg::InstantiateMsg {
        root: CREATOR_ADDR.to_string(),
        dao_code_hash: gov_info.code_hash.clone(),
    };
    let voting_instantiate = dao_voting_snip20_balance::msg::InstantiateMsg {
        token_info: dao_voting_snip20_balance::msg::TokenInfo::New {
            code_id: snip20_info.code_id.clone(),
            code_hash: snip20_info.code_hash.clone(),
            label: "DAO DAO voting".to_string(),
            name: "DAO DAO".to_string(),
            symbol: "DAO".to_string(),
            decimals: 6,
            initial_balances: vec![InitialBalance {
                address: CREATOR_ADDR.to_string(),
                amount: Uint128::from(2u64),
            }],
        },
        dao_code_hash: gov_info.code_hash.clone(),
    };

    let gov_instantiate = InstantiateMsg {
        dao_uri: None,
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs.".to_string(),
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: voting_info.code_id,
            code_hash: voting_info.code_hash.clone(),
            msg: to_binary(&voting_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: govmod_info.code_id,
            code_hash: govmod_info.code_hash.clone(),
            msg: to_binary(&govmod_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "governance module".to_string(),
        }],
        initial_items: None,
        query_auth_code_id: query_auth_info.code_id,
        query_auth_code_hash: query_auth_info.code_hash,
        prng_seed: "seed".to_string(),
    };

    let gov_contract_info = app
        .instantiate_contract(
            gov_info.clone(),
            Addr::unchecked(CREATOR_ADDR),
            &gov_instantiate,
            &[],
            "cw-governance",
            None,
        )
        .unwrap();

    set_item(
        &mut app,
        gov_contract_info.clone(),
        "fookey".to_string(),
        "fooaddr".to_string(),
    );
    set_item(
        &mut app,
        gov_contract_info.clone(),
        "barkey".to_string(),
        "baraddr".to_string(),
    );
    set_item(
        &mut app,
        gov_contract_info.clone(),
        "loremkey".to_string(),
        "loremaddr".to_string(),
    );
    set_item(
        &mut app,
        gov_contract_info.clone(),
        "ipsumkey".to_string(),
        "ipsumaddr".to_string(),
    );

    // Foo returned as we are only getting one item and items are in
    // decending order.
    let first_item = list_items(&mut app, gov_contract_info.clone(), None, Some(1));
    assert_eq!(first_item.len(), 1);
    assert_eq!(
        first_item[0],
        ("loremkey".to_string(), "loremaddr".to_string())
    );

    let no_items = list_items(&mut app, gov_contract_info.clone(), None, Some(0));
    assert_eq!(no_items.len(), 0);

    // Items are retreived in decending order so asking for foo with
    // no limit ought to give us the barkey k/v. this will be the last item
    // note: the paginate map bound is exclusive, so fookey will be starting point
    let last_item = list_items(
        &mut app,
        gov_contract_info.clone(),
        Some("fookey".to_string()),
        None,
    );
    assert_eq!(last_item.len(), 1);
    assert_eq!(last_item[0], ("barkey".to_string(), "baraddr".to_string()));

    // Items are retreived in decending order so asking for ipsum with
    // 4 limit ought to give us the fookey and barkey k/vs.
    let after_foo_list = list_items(
        &mut app,
        gov_contract_info,
        Some("ipsumkey".to_string()),
        Some(4),
    );
    assert_eq!(after_foo_list.len(), 2);
    assert_eq!(
        after_foo_list,
        vec![
            ("fookey".to_string(), "fooaddr".to_string()),
            ("barkey".to_string(), "baraddr".to_string())
        ]
    );
}

#[test]
fn test_instantiate_with_items() {
    let mut app = App::default();
    let govmod_info = app.store_code(sudo_proposal_contract());
    let voting_info = app.store_code(snip20_balances_voting());
    let gov_info = app.store_code(cw_core_contract());
    let snip20_info = app.store_code(snip20_contract());
    let query_auth_info = app.store_code(query_auth_contract());

    let govmod_instantiate = dao_proposal_sudo::msg::InstantiateMsg {
        root: CREATOR_ADDR.to_string(),
        dao_code_hash: gov_info.code_hash.clone(),
    };
    let voting_instantiate = dao_voting_snip20_balance::msg::InstantiateMsg {
        token_info: dao_voting_snip20_balance::msg::TokenInfo::New {
            code_id: snip20_info.code_id.clone(),
            code_hash: snip20_info.code_hash.clone(),
            label: "DAO DAO voting".to_string(),
            name: "DAO DAO".to_string(),
            symbol: "DAO".to_string(),
            decimals: 6,
            initial_balances: vec![InitialBalance {
                address: CREATOR_ADDR.to_string(),
                amount: Uint128::from(2u64),
            }],
        },
        dao_code_hash: gov_info.code_hash.clone(),
    };

    let mut initial_items = vec![
        InitialItem {
            key: "item0".to_string(),
            value: "item0_value".to_string(),
        },
        InitialItem {
            key: "item1".to_string(),
            value: "item1_value".to_string(),
        },
        InitialItem {
            key: "item0".to_string(),
            value: "item0_value_override".to_string(),
        },
    ];

    let mut gov_instantiate = InstantiateMsg {
        dao_uri: None,
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs.".to_string(),
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: voting_info.code_id,
            code_hash: voting_info.code_hash.clone(),
            msg: to_binary(&voting_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: govmod_info.code_id,
            code_hash: govmod_info.code_hash.clone(),
            msg: to_binary(&govmod_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "governance module".to_string(),
        }],
        initial_items: Some(initial_items.clone()),
        query_auth_code_id: query_auth_info.code_id,
        query_auth_code_hash: query_auth_info.code_hash,
        prng_seed: "seed".to_string(),
    };

    // Ensure duplicates are dissallowed.
    let err: ContractError = app
        .instantiate_contract(
            gov_info.clone(),
            Addr::unchecked(CREATOR_ADDR),
            &gov_instantiate,
            &[],
            "cw-governance",
            None,
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(
        err,
        ContractError::DuplicateInitialItem {
            item: "item0".to_string()
        }
    );

    initial_items.pop();
    gov_instantiate.initial_items = Some(initial_items);
    let gov_contract_info = app
        .instantiate_contract(
            gov_info.clone(),
            Addr::unchecked(CREATOR_ADDR),
            &gov_instantiate,
            &[],
            "cw-governance",
            None,
        )
        .unwrap();

    // Ensure initial items were added.
    let items = list_items(&mut app, gov_contract_info.clone(), None, None);
    assert_eq!(items.len(), 2);

    // Descending order, so item1 is first.
    assert_eq!(items[1].0, "item0".to_string());
    let get_item0 = get_item(&mut app, gov_contract_info.clone(), "item0".to_string());
    assert_eq!(
        get_item0,
        GetItemResponse {
            item: Some("item0_value".to_string()),
        }
    );

    assert_eq!(items[0].0, "item1".to_string());
    let item1_value = get_item(&mut app, gov_contract_info, "item1".to_string()).item;
    assert_eq!(item1_value, Some("item1_value".to_string()))
}

#[test]
fn test_snip20_receive_auto_add() {
    let (gov_contract_info, mut app) = do_standard_instantiate(true, None);

    let snip20_info = app.store_code(snip20_contract());
    let another_snip20_contract = app
        .instantiate_contract(
            snip20_info,
            Addr::unchecked(CREATOR_ADDR),
            &snip20_base::msg::InstantiateMsg {
                name: "DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![].into(),
                admin: None,
                prng_seed: to_binary(&"seeed").unwrap(),
                config: None,
                supported_denoms: None,
            },
            &[],
            "another-token",
            None,
        )
        .unwrap();

    let voting_module: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash.clone(),
            gov_contract_info.address.clone(),
            &QueryMsg::VotingModule {},
        )
        .unwrap();
    let gov_token_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_module.code_hash.clone(),
            voting_module.addr.clone(),
            &dao_interface::voting::Query::TokenContract {},
        )
        .unwrap();

    // Check that the balances query works with no tokens.
    let snip20_balances: Vec<Snip20BalanceResponse> = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash.clone(),
            gov_contract_info.address.clone(),
            &QueryMsg::Snip20Balances {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(snip20_balances, vec![]);

    // Send a gov token to the governance contract.
    app.execute_contract(
        Addr::unchecked(CREATOR_ADDR),
        &ContractInfo {
            address: gov_token_info.addr.clone(),
            code_hash: gov_token_info.code_hash.clone(),
        },
        &snip20_base::msg::ExecuteMsg::Send {
            recipient: gov_contract_info.address.clone().to_string(),
            recipient_code_hash: Some(gov_contract_info.code_hash.clone()),
            amount: Uint128::new(1),
            msg: Some(to_binary(&"").unwrap()),
            memo: None,
            decoys: None,
            entropy: None,
            padding: None,
        },
        &[],
    )    .unwrap();

    let snip20_list: Vec<Addr> = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash.clone(),
            gov_contract_info.address.clone(),
            &QueryMsg::Snip20TokenList {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(snip20_list, vec![gov_token_info.addr.clone()]);

    let snip20_balances: Vec<Snip20BalanceResponse> = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash.clone(),
            gov_contract_info.address.clone(),
            &QueryMsg::Snip20Balances {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(
        snip20_balances,
        vec![Snip20BalanceResponse {
            addr: gov_token_info.addr.clone().to_string(),
            balance: Uint128::new(1),
        }]
    );

    // Test removing and adding some new ones. Invalid should fail.
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(gov_contract_info.address.clone()),
            &gov_contract_info.clone(),
            &ExecuteMsg::UpdateSnip20List {
                to_add: vec!["new".to_string()],
                to_remove: vec![gov_token_info.addr.clone().to_string()],
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert!(matches!(err, ContractError::Std(_)));

    // Test that non-DAO can not update the list.
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("ekez"),
            &gov_contract_info.clone(),
            &ExecuteMsg::UpdateSnip20List {
                to_add: vec![],
                to_remove: vec![gov_token_info.addr.clone().to_string()],
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert!(matches!(err, ContractError::Unauthorized {}));

    app.execute_contract(
        Addr::unchecked(gov_contract_info.address.clone()),
        &gov_contract_info.clone(),
        &ExecuteMsg::UpdateSnip20List {
            to_add: vec![another_snip20_contract.address.to_string()],
            to_remove: vec![gov_token_info.addr.clone().to_string()],
        },
        &[],
    )
    .unwrap();

    let snip20_list: Vec<Addr> = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash,
            gov_contract_info.address,
            &QueryMsg::Snip20TokenList {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(snip20_list, vec![another_snip20_contract.address]);
}

#[test]
fn test_snip721_receive() {
    let (gov_contract_info, mut app) = do_standard_instantiate(true, None);

    let snip721_info = app.store_code(snip721_contract());

    let snip721_contract_info = app
        .instantiate_contract(
            snip721_info.clone(),
            Addr::unchecked(CREATOR_ADDR),
            &snip721_reference_impl::msg::InstantiateMsg {
                name: "ekez".to_string(),
                symbol: "ekez".to_string(),
                admin: None,
                entropy: "entropy".to_string(),
                royalty_info: None,
                config: None,
                post_init_callback: None,
            },
            &[],
            "snip721",
            None,
        )
        .unwrap();

    let another_snip721 = app
        .instantiate_contract(
            snip721_info.clone(),
            Addr::unchecked(CREATOR_ADDR),
            &snip721_reference_impl::msg::InstantiateMsg {
                name: "ekez".to_string(),
                symbol: "ekez".to_string(),
                admin: None,
                entropy: "entropy".to_string(),
                royalty_info: None,
                config: None,
                post_init_callback: None,
            },
            &[],
            "snip721",
            None,
        )
        .unwrap();

    app.execute_contract(
        Addr::unchecked(CREATOR_ADDR),
        &snip721_contract_info.clone(),
        &snip721_reference_impl::msg::ExecuteMsg::MintNft {
            token_id: Some("ekez".to_string()),
            owner: Some(CREATOR_ADDR.to_string()),
            public_metadata: None,
            private_metadata: None,
            serial_number: None,
            royalty_info: None,
            transferable: Some(true),
            memo: None,
            padding: None,
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        Addr::unchecked(CREATOR_ADDR),
        &snip721_contract_info.clone(),
        &snip721_reference_impl::msg::ExecuteMsg::SendNft {
            contract: gov_contract_info.address.to_string(),
            token_id: "ekez".to_string(),
            msg: Some(to_binary("").unwrap()),
            receiver_info: Some(ReceiverInfo {
                recipient_code_hash: gov_contract_info.code_hash.clone(),
                also_implements_batch_receive_nft: Some(true),
            }),
            memo: None,
            padding: None,
        },
        &[],
    )
    .unwrap();

    let snip721_list: Vec<Addr> = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash.clone(),
            gov_contract_info.address.clone(),
            &QueryMsg::Snip721TokenList {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(snip721_list, vec![CREATOR_ADDR.to_string()]);

    // Try to add an invalid snip721.
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(gov_contract_info.address.clone()),
            &gov_contract_info.clone(),
            &ExecuteMsg::UpdateSnip721List {
                to_add: vec!["new".to_string(), snip721_contract_info.address.to_string()],
                to_remove: vec![snip721_contract_info.address.to_string()],
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert!(matches!(err, ContractError::Std(_)));

    // Test that non-DAO can not update the list.
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("ekez"),
            &gov_contract_info.clone(),
            &ExecuteMsg::UpdateSnip721List {
                to_add: vec![],
                to_remove: vec![snip721_contract_info.address.clone().to_string()],
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert!(matches!(err, ContractError::Unauthorized {}));

    // Add a real snip721.
    app.execute_contract(
        Addr::unchecked(gov_contract_info.address.clone()),
        &gov_contract_info.clone(),
        &ExecuteMsg::UpdateSnip721List {
            to_add: vec![another_snip721.address.to_string()],
            to_remove: vec![CREATOR_ADDR.to_string()],
        },
        &[],
    )
    .unwrap();

    let snip20_list: Vec<Addr> = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash,
            gov_contract_info.address,
            &QueryMsg::Snip721TokenList {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(snip20_list, vec![another_snip721.address]);
}

#[test]
fn test_pause() {
    let (core_contract_info, mut app) = do_standard_instantiate(false, None);

    let start_height = app.block_info().height;

    let proposal_modules: Vec<ProposalModule> = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::ProposalModules {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(proposal_modules.len(), 1);
    let proposal_module = proposal_modules.into_iter().next().unwrap();

    let paused: PauseInfoResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::PauseInfo {},
        )
        .unwrap();
    assert_eq!(paused, PauseInfoResponse::Unpaused {});
    let all_state: DumpStateResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::DumpState {},
        )
        .unwrap();
    assert_eq!(all_state.pause_info, PauseInfoResponse::Unpaused {});

    // DAO is not paused. Check that we can execute things.
    //
    // Tests intentionally use the core address to send these
    // messsages to simulate a worst case scenerio where the core
    // contract has a vulnerability.
    app.execute_contract(
        core_contract_info.address.clone(),
        &core_contract_info.clone(),
        &ExecuteMsg::UpdateConfig {
            config: Config {
                dao_uri: None,
                name: "The Empire Strikes Back".to_string(),
                description: "haha lol we have pwned your DAO".to_string(),
                image_url: None,
            },
        },
        &[],
    )
    .unwrap();

    // Oh no the DAO is under attack! Quick! Pause the DAO while we
    // figure out what to do!
    let err: ContractError = app
        .execute_contract(
            proposal_module.address.clone(),
            &core_contract_info.clone(),
            &ExecuteMsg::Pause {
                duration: Duration::Height(10),
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    // Only the DAO may call this on itself. Proposal modules must use
    // the execute hook.
    assert_eq!(err, ContractError::Unauthorized {});

    app.execute_contract(
        proposal_module.address.clone(),
        &core_contract_info.clone(),
        &ExecuteMsg::ExecuteProposalHook {
            msgs: vec![WasmMsg::Execute {
                contract_addr: core_contract_info.address.clone().to_string(),
                code_hash: core_contract_info.code_hash.clone(),
                msg: to_binary(&ExecuteMsg::Pause {
                    duration: Duration::Height(10),
                })
                .unwrap(),
                funds: vec![],
            }
            .into()],
        },
        &[],
    )
    .unwrap();

    let paused: PauseInfoResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::PauseInfo {},
        )
        .unwrap();
    assert_eq!(
        paused,
        PauseInfoResponse::Paused {
            expiration: Expiration::AtHeight(start_height + 10)
        }
    );
    let all_state: DumpStateResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::DumpState {},
        )
        .unwrap();
    assert_eq!(
        all_state.pause_info,
        PauseInfoResponse::Paused {
            expiration: Expiration::AtHeight(start_height + 10)
        }
    );

    let err: ContractError = app
        .execute_contract(
            core_contract_info.address.clone(),
            &core_contract_info.clone(),
            &ExecuteMsg::UpdateConfig {
                config: Config {
                    dao_uri: None,
                    name: "The Empire Strikes Back Again".to_string(),
                    description: "haha lol we have pwned your DAO again".to_string(),
                    image_url: None,
                },
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    assert!(matches!(err, ContractError::Paused { .. }));

    let err: ContractError = app
        .execute_contract(
            proposal_module.address.clone(),
            &core_contract_info.clone(),
            &ExecuteMsg::ExecuteProposalHook {
                msgs: vec![WasmMsg::Execute {
                    contract_addr: core_contract_info.address.clone().to_string(),
                    code_hash: core_contract_info.code_hash.clone(),
                    msg: to_binary(&ExecuteMsg::Pause {
                        duration: Duration::Height(10),
                    })
                    .unwrap(),
                    funds: vec![],
                }
                .into()],
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    assert!(matches!(err, ContractError::Paused { .. }));

    app.update_block(|block| block.height += 9);

    // Still not unpaused.
    let err: ContractError = app
        .execute_contract(
            proposal_module.address.clone(),
            &core_contract_info.clone(),
            &ExecuteMsg::ExecuteProposalHook {
                msgs: vec![WasmMsg::Execute {
                    contract_addr: core_contract_info.address.clone().to_string(),
                    code_hash: core_contract_info.code_hash.clone(),
                    msg: to_binary(&ExecuteMsg::Pause {
                        duration: Duration::Height(10),
                    })
                    .unwrap(),
                    funds: vec![],
                }
                .into()],
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    assert!(matches!(err, ContractError::Paused { .. }));

    app.update_block(|block| block.height += 1);

    let paused: PauseInfoResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::PauseInfo {},
        )
        .unwrap();
    assert_eq!(paused, PauseInfoResponse::Unpaused {});
    let all_state: DumpStateResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::DumpState {},
        )
        .unwrap();
    assert_eq!(all_state.pause_info, PauseInfoResponse::Unpaused {});

    // Now its unpaused so we should be able to pause again.
    app.execute_contract(
        proposal_module.address,
        &core_contract_info.clone(),
        &ExecuteMsg::ExecuteProposalHook {
            msgs: vec![WasmMsg::Execute {
                contract_addr: core_contract_info.address.clone().to_string(),
                code_hash: core_contract_info.code_hash.clone(),
                msg: to_binary(&ExecuteMsg::Pause {
                    duration: Duration::Height(10),
                })
                .unwrap(),
                funds: vec![],
            }
            .into()],
        },
        &[],
    )
    .unwrap();

    let paused: PauseInfoResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::PauseInfo {},
        )
        .unwrap();
    assert_eq!(
        paused,
        PauseInfoResponse::Paused {
            expiration: Expiration::AtHeight(start_height + 20)
        }
    );
    let all_state: DumpStateResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash,
            core_contract_info.address,
            &QueryMsg::DumpState {},
        )
        .unwrap();
    assert_eq!(
        all_state.pause_info,
        PauseInfoResponse::Paused {
            expiration: Expiration::AtHeight(start_height + 20)
        }
    );
}

#[test]
fn test_dump_state_proposal_modules() {
    let (core_contract_info, app) = do_standard_instantiate(false, None);
    let proposal_modules: Vec<ProposalModule> = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::ProposalModules {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(proposal_modules.len(), 1);
    let proposal_module = proposal_modules.into_iter().next().unwrap();

    let all_state: DumpStateResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash,
            core_contract_info.address,
            &QueryMsg::DumpState {},
        )
        .unwrap();
    assert_eq!(all_state.pause_info, PauseInfoResponse::Unpaused {});
    assert_eq!(all_state.proposal_modules.len(), 1);
    assert_eq!(all_state.proposal_modules[0], proposal_module);
}

#[test]
fn test_execute_stargate_msg() {
    let (core_contract_info, mut app) = do_standard_instantiate(true, None);
    let proposal_modules: Vec<ProposalModule> = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::ProposalModules {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(proposal_modules.len(), 1);
    let proposal_module = proposal_modules.into_iter().next().unwrap();

    let res = app.execute_contract(
        proposal_module.address,
        &core_contract_info,
        &ExecuteMsg::ExecuteProposalHook {
            msgs: vec![CosmosMsg::Stargate {
                type_url: "foo_type".to_string(),
                value: to_binary("foo_bin").unwrap(),
            }],
        },
        &[],
    );
    // TODO: Once cw-multi-test supports executing stargate/ibc messages we can change this test assert
    assert!(res.is_err());
}

#[test]
fn test_module_prefixes() {
    let mut app = App::default();
    let govmod_info = app.store_code(sudo_proposal_contract());
    let gov_info = app.store_code(cw_core_contract());
    let query_auth_info = app.store_code(query_auth_contract());

    let govmod_instantiate = dao_proposal_sudo::msg::InstantiateMsg {
        root: CREATOR_ADDR.to_string(),
        dao_code_hash: gov_info.code_hash.clone(),
    };

    let gov_instantiate = InstantiateMsg {
        dao_uri: None,
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs.".to_string(),
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: govmod_info.code_id,
            code_hash: govmod_info.code_hash.clone(),
            msg: to_binary(&govmod_instantiate).unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![
            ModuleInstantiateInfo {
                code_id: govmod_info.code_id,
                code_hash: govmod_info.code_hash.clone(),
                msg: to_binary(&govmod_instantiate).unwrap(),
                admin: Some(Admin::CoreModule {}),
                funds: vec![],
                label: "proposal module 1".to_string(),
            },
            ModuleInstantiateInfo {
                code_id: govmod_info.code_id,
                code_hash: govmod_info.code_hash.clone(),
                msg: to_binary(&govmod_instantiate).unwrap(),
                admin: Some(Admin::CoreModule {}),
                funds: vec![],
                label: "proposal module 2".to_string(),
            },
            ModuleInstantiateInfo {
                code_id: govmod_info.code_id,
                code_hash: govmod_info.code_hash.clone(),
                msg: to_binary(&govmod_instantiate).unwrap(),
                admin: Some(Admin::CoreModule {}),
                funds: vec![],
                label: "proposal module 2".to_string(),
            },
        ],
        initial_items: None,
        query_auth_code_id: query_auth_info.code_id,
        query_auth_code_hash: query_auth_info.code_hash,
        prng_seed: "Seeed".to_string(),
    };

    let gov_contract_info = app
        .instantiate_contract(
            gov_info,
            Addr::unchecked(CREATOR_ADDR),
            &gov_instantiate,
            &[],
            "cw-governance",
            None,
        )
        .unwrap();

    let modules: Vec<ProposalModule> = app
        .wrap()
        .query_wasm_smart(
            gov_contract_info.code_hash.clone(),
            gov_contract_info.address.clone(),
            &QueryMsg::ProposalModules {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(modules.len(), 3);

    let module_1 = &modules[0];
    assert_eq!(module_1.status, ProposalModuleStatus::Enabled {});
    assert_eq!(module_1.prefix, "A");
    assert_eq!(&module_1.address, &modules[0].address);

    let module_2 = &modules[1];
    assert_eq!(module_2.status, ProposalModuleStatus::Enabled {});
    assert_eq!(module_2.prefix, "B");
    assert_eq!(&module_2.address, &modules[1].address);

    let module_3 = &modules[2];
    assert_eq!(module_3.status, ProposalModuleStatus::Enabled {});
    assert_eq!(module_3.prefix, "C");
    assert_eq!(&module_3.address, &modules[2].address);
}

fn get_active_modules(app: &App, gov_info: ContractInfo) -> Vec<ProposalModule> {
    let modules: Vec<ProposalModule> = app
        .wrap()
        .query_wasm_smart(
            gov_info.code_hash,
            gov_info.address,
            &QueryMsg::ProposalModules {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    modules
        .into_iter()
        .filter(|module: &ProposalModule| module.status == ProposalModuleStatus::Enabled)
        .collect()
}

fn query_proposal_module_count(app: &App, core_info: &ContractInfo) -> ProposalModuleCountResponse {
    app.wrap()
        .query_wasm_smart(
            core_info.code_hash.clone(),
            core_info.address.clone(),
            &QueryMsg::ProposalModuleCount {},
        )
        .unwrap()
}

#[test]
fn test_add_remove_subdaos() {
    let (core_contract_info, mut app) = do_standard_instantiate(false, None);

    test_unauthorized(
        &mut app,
        core_contract_info.clone(),
        ExecuteMsg::UpdateSubDaos {
            to_add: vec![],
            to_remove: vec![],
        },
    );

    let to_add: Vec<SubDao> = vec![
        SubDao {
            addr: "subdao001".to_string(),
            code_hash: "subdao001_code_hash".to_string(),
            charter: None,
        },
        SubDao {
            addr: "subdao002".to_string(),
            code_hash: "subdao002_code_hash".to_string(),
            charter: Some("cool charter bro".to_string()),
        },
        SubDao {
            addr: "subdao005".to_string(),
            code_hash: "subdao005_code_hash".to_string(),
            charter: None,
        },
        SubDao {
            addr: "subdao007".to_string(),
            code_hash: "subdao007_code_hash".to_string(),
            charter: None,
        },
    ];
    let to_remove: Vec<String> = vec![];

    app.execute_contract(
        Addr::unchecked(core_contract_info.address.clone()),
        &core_contract_info.clone(),
        &ExecuteMsg::UpdateSubDaos { to_add, to_remove },
        &[],
    )
    .unwrap();

    let res: Vec<SubDao> = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::ListSubDaos {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(res.len(), 4);

    let to_remove: Vec<String> = vec!["subdao005".to_string()];

    app.execute_contract(
        Addr::unchecked(core_contract_info.address.clone()),
        &core_contract_info.clone(),
        &ExecuteMsg::UpdateSubDaos {
            to_add: vec![],
            to_remove,
        },
        &[],
    )
    .unwrap();

    let res: Vec<SubDao> = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &QueryMsg::ListSubDaos {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(res.len(), 3);

    let test_res: SubDao = SubDao {
        addr: "subdao002".to_string(),
        code_hash: "subdao002_code_hash".to_string(),
        charter: Some("cool charter bro".to_string()),
    };

    assert_eq!(res[1], test_res);

    let full_result_set: Vec<SubDao> = vec![
        SubDao {
            addr: "subdao001".to_string(),
            code_hash: "subdao001_code_hash".to_string(),
            charter: None,
        },
        SubDao {
            addr: "subdao002".to_string(),
            code_hash: "subdao002_code_hash".to_string(),
            charter: Some("cool charter bro".to_string()),
        },
        SubDao {
            addr: "subdao007".to_string(),
            code_hash: "subdao007_code_hash".to_string(),
            charter: None,
        },
    ];

    assert_eq!(res, full_result_set);
}

#[test]
pub fn test_migrate_update_version() {
    let mut deps = mock_dependencies();
    secret_cw2::set_contract_version(&mut deps.storage, "my-contract", "1.0.0").unwrap();
    migrate(deps.as_mut(), mock_env(), MigrateMsg {}).unwrap();
    let version = secret_cw2::get_contract_version(&deps.storage).unwrap();
    assert_eq!(version.version, CONTRACT_VERSION);
    assert_eq!(version.contract, CONTRACT_NAME);
}

#[test]
fn test_query_info() {
    let (core_contract_info, app) = do_standard_instantiate(true, None);
    let res: InfoResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash,
            core_contract_info.address,
            &QueryMsg::Info {},
        )
        .unwrap();
    assert_eq!(
        res,
        InfoResponse {
            info: ContractVersion {
                contract: CONTRACT_NAME.to_string(),
                version: CONTRACT_VERSION.to_string()
            }
        }
    )
}
