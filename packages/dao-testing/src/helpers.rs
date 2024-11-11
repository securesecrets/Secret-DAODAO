use cosmwasm_std::{from_binary, to_binary, Addr, Binary, ContractInfo, Uint128};
use dao_interface::{
    msg::InitialBalance,
    state::{Admin, AnyContractInfo, ModuleInstantiateInfo},
};
use dao_voting::threshold::ActiveThreshold;
use dao_voting_cw4::msg::GroupContract;
use secret_multi_test::{App, Executor};
use secret_utils::Duration;
use shade_protocol::basic_staking::Auth;

use crate::contracts::{
    cw4_group_contract, dao_dao_contract, dao_voting_cw4_contract, query_auth_contract,
    snip20_base_contract, snip20_stake_contract, snip20_staked_balances_voting_contract,
    snip721_base_contract,
};

const CREATOR_ADDR: &str = "creator";

// pub fn instantiate_with_cw20_balances_governance(
//     app: &mut App,
//     governance_code_id: u64,
//     governance_instantiate: Binary,
//     initial_balances: Option<Vec<InitialBalance>>,
// ) -> Addr {
//     let snip20_info = app.store_code(snip20_base_contract());
//     let core_id = app.store_code(dao_dao_contract());
//     let votemod_info = app.store_code(cw20_balances_voting_contract());

//     let initial_balances = initial_balances.unwrap_or_else(|| {
//         vec![InitialBalance {
//             address: CREATOR_ADDR.to_string(),
//             amount: Uint128::new(100_000_000),
//         }]
//     });

//     // Collapse balances so that we can test double votes.
//     let initial_balances: Vec<InitialBalance> = {
//         let mut already_seen = vec![];
//         initial_balances
//             .into_iter()
//             .filter(|InitialBalance { address, amount: _ }| {
//                 if already_seen.contains(address) {
//                     false
//                 } else {
//                     already_seen.push(address.clone());
//                     true
//                 }
//             })
//             .collect()
//     };

//     let governance_instantiate = dao_interface::msg::InstantiateMsg {
//         dao_uri: None,
//         admin: None,
//         name: "DAO DAO".to_string(),
//         description: "A DAO that builds DAOs".to_string(),
//         image_url: None,
//         automatically_add_cw20s: true,
//         automatically_add_cw721s: true,
//         voting_module_instantiate_info: ModuleInstantiateInfo {
//             code_id: votemod_info,
//             msg: to_binary(&dao_voting_cw20_balance::msg::InstantiateMsg {
//                 token_info: dao_voting_cw20_balance::msg::TokenInfo::New {
//                     code_id: snip20_info,
//                     label: "DAO DAO governance token".to_string(),
//                     name: "DAO".to_string(),
//                     symbol: "DAO".to_string(),
//                     decimals: 6,
//                     initial_balances,
//                     marketing: None,
//                 },
//             })
//             .unwrap(),
//             admin: Some(Admin::CoreModule {}),
//             funds: vec![],
//             label: "DAO DAO voting module".to_string(),
//         },
//         proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
//             code_id: governance_code_id,
//             msg: governance_instantiate,
//             admin: Some(Admin::CoreModule {}),
//             funds: vec![],
//             label: "DAO DAO governance module".to_string(),
//         }],
//         initial_items: None,
//     };

//     app.instantiate_contract(
//         core_id,
//         Addr::unchecked(CREATOR_ADDR),
//         &governance_instantiate,
//         &[],
//         "DAO DAO",
//         None,
//     )
//     .unwrap()
// }

pub fn create_viewing_key(app: &mut App, contract_info: ContractInfo, sender: &str) -> String {
    let msg = shade_protocol::contract_interfaces::query_auth::ExecuteMsg::CreateViewingKey {
        entropy: "entropy".to_string(),
        padding: None,
    };
    let res = app
        .execute_contract(Addr::unchecked(sender), &contract_info, &msg, &[])
        .unwrap();
    let mut viewing_key = String::new();
    let data: shade_protocol::contract_interfaces::query_auth::ExecuteAnswer =
        from_binary(&res.data.unwrap()).unwrap();
    if let shade_protocol::contract_interfaces::query_auth::ExecuteAnswer::CreateViewingKey {
        key,
    } = data
    {
        viewing_key = key;
    };
    viewing_key
}

pub fn instantiate_with_staked_balances_governance(
    app: &mut App,
    governance_code_id: u64,
    governance_code_hash: String,
    governance_instantiate: Binary,
    initial_balances: Option<Vec<InitialBalance>>,
) -> ContractInfo {
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
    let snip721_info = app.store_code(snip721_base_contract());
    let snip20_stake_info = app.store_code(snip20_stake_contract());
    let staked_balances_voting_info = app.store_code(snip20_staked_balances_voting_contract());
    let core_contract_info = app.store_code(dao_dao_contract());
    let query_auth_info = app.store_code(query_auth_contract());

    let instantiate_core = dao_interface::msg::InstantiateMsg {
        dao_uri: None,
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
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
                dao_code_hash: core_contract_info.code_hash.clone(),
                query_auth: None,
            })
            .unwrap(),
            admin: None,
            funds: vec![],
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: governance_code_id,
            code_hash: governance_code_hash,
            label: "DAO DAO governance module.".to_string(),
            admin: Some(Admin::CoreModule {}),
            msg: governance_instantiate,
            funds: vec![],
        }],
        initial_items: None,
        query_auth_code_id: query_auth_info.code_id,
        query_auth_code_hash: query_auth_info.code_hash,
        prng_seed: "Seed".into(),
        snip20_code_hash: snip20_info.code_hash.clone(),
        snip721_code_hash: snip721_info.code_hash,
    };

    let core_info = app
        .instantiate_contract(
            core_contract_info,
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
            core_info.code_hash.clone(),
            core_info.address.clone(),
            &dao_interface::msg::QueryMsg::DumpState {},
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

    let query_auth: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            core_info.code_hash.clone(),
            core_info.address.clone(),
            &dao_interface::msg::QueryMsg::QueryAuthInfo {},
        )
        .unwrap();

    // Stake all the initial balances.
    for InitialBalance { address, amount } in initial_balances {
        let viewing_key = create_viewing_key(
            app,
            ContractInfo {
                address: query_auth.addr.clone(),
                code_hash: query_auth.code_hash.clone(),
            },
            &address.clone(),
        );
        app.execute_contract(
            Addr::unchecked(address.clone()),
            &ContractInfo {
                address: token_contract.addr.clone(),
                code_hash: token_contract.code_hash.clone(),
            },
            &snip20_reference_impl::msg::ExecuteMsg::Send {
                recipient: staking_contract.addr.clone().to_string(),
                recipient_code_hash: Some(staking_contract.code_hash.clone()),
                amount,
                msg: Some(
                    to_binary(&snip20_stake::msg::ReceiveMsg::Stake {
                        auth: Box::new(Auth::ViewingKey {
                            key: viewing_key.clone(),
                            address,
                        }),
                    })
                    .unwrap(),
                ),
                decoys: None,
                memo: None,
                entropy: None,
                padding: None,
            },
            &[],
        )
        .unwrap();
    }

    // Update the block so that those staked balances appear.
    app.update_block(|block| block.height += 1);

    core_info
}

pub fn instantiate_with_staking_active_threshold(
    app: &mut App,
    code_id: u64,
    code_hash: String,
    governance_instantiate: Binary,
    initial_balances: Option<Vec<InitialBalance>>,
    active_threshold: Option<ActiveThreshold>,
) -> ContractInfo {
    let snip20_info = app.store_code(snip20_base_contract());
    let snip721_info = app.store_code(snip721_base_contract());
    let snip20_staking_info = app.store_code(snip20_stake_contract());
    let governance_info = app.store_code(dao_dao_contract());
    let votemod_info = app.store_code(snip20_staked_balances_voting_contract());
    let query_auth = app.store_code(query_auth_contract());

    let initial_balances = initial_balances.unwrap_or_else(|| {
        vec![
            InitialBalance {
                address: "blob".to_string(),
                amount: Uint128::new(100_000_000),
            },
            InitialBalance {
                address: "blue".to_string(),
                amount: Uint128::new(100_000_000),
            },
        ]
    });

    let governance_instantiate = dao_interface::msg::InstantiateMsg {
        dao_uri: None,
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: votemod_info.code_id,
            code_hash: votemod_info.code_hash,
            msg: to_binary(&dao_voting_snip20_staked::msg::InstantiateMsg {
                token_info: dao_voting_snip20_staked::msg::Snip20TokenInfo::New {
                    code_id: snip20_info.code_id,
                    code_hash: snip20_info.code_hash.clone(),
                    name: "DAO".to_string(),
                    symbol: "DAO".to_string(),
                    decimals: 6,
                    initial_balances,
                    staking_code_id: snip20_staking_info.code_id,
                    staking_code_hash: snip20_staking_info.code_hash,
                    unstaking_duration: None,
                    initial_dao_balance: None,
                },
                active_threshold,
                dao_code_hash: governance_info.code_hash.clone(),
                query_auth: None,
            })
            .unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id,
            code_hash,
            msg: governance_instantiate,
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO governance module".to_string(),
        }],
        initial_items: None,
        query_auth_code_id: query_auth.code_id,
        query_auth_code_hash: query_auth.code_hash,
        prng_seed: "Seed".to_string(),
        snip20_code_hash: snip20_info.code_hash,
        snip721_code_hash: snip721_info.code_hash,
    };

    app.instantiate_contract(
        governance_info,
        Addr::unchecked(CREATOR_ADDR),
        &governance_instantiate,
        &[],
        "DAO DAO",
        None,
    )
    .unwrap()
}

pub fn instantiate_with_cw4_groups_governance(
    app: &mut App,
    core_code_id: u64,
    core_code_hash: String,
    proposal_module_instantiate: Binary,
    initial_weights: Option<Vec<InitialBalance>>,
) -> ContractInfo {
    let cw4_info = app.store_code(cw4_group_contract());
    let core_info = app.store_code(dao_dao_contract());
    let votemod_info = app.store_code(dao_voting_cw4_contract());
    let snip20_info = app.store_code(snip20_base_contract());
    let snip721_info = app.store_code(snip721_base_contract());
    let query_auth = app.store_code(query_auth_contract());
    let initial_weights = initial_weights.unwrap_or_default();

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
        dao_uri: None,
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: votemod_info.code_id,
            code_hash: votemod_info.code_hash,
            msg: to_binary(&dao_voting_cw4::msg::InstantiateMsg {
                group_contract: GroupContract::New {
                    cw4_group_code_id: cw4_info.code_id,
                    cw4_group_code_hash: cw4_info.code_hash,
                    initial_members: initial_weights,
                    query_auth: None,
                },
                dao_code_hash: core_info.code_hash.clone(),
            })
            .unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: core_code_id,
            code_hash: core_code_hash,
            msg: proposal_module_instantiate,
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO governance module".to_string(),
        }],
        initial_items: None,
        query_auth_code_id: query_auth.code_id,
        query_auth_code_hash: query_auth.code_hash,
        prng_seed: "seed".into(),
        snip20_code_hash: snip20_info.code_hash,
        snip721_code_hash: snip721_info.code_hash,
    };

    let addr = app
        .instantiate_contract(
            core_info.clone(),
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
