use cosmwasm_std::{
    from_binary,
    testing::{mock_dependencies, mock_env, mock_info},
    to_binary, Addr, ContractInfo, CosmosMsg, Empty, MessageInfo, Uint128, WasmMsg,
};
use dao_interface::{
    state::AnyContractInfo,
    voting::{InfoResponse, TotalPowerAtHeightResponse, VotingPowerAtHeightResponse},
};
use secret_cw2::ContractVersion;
use secret_multi_test::{
    next_block, App, Contract, ContractInstantiationInfo, ContractWrapper, Executor,
};
use shade_protocol::{basic_staking::Auth, utils::asset::RawContract};

use crate::{
    contract::{migrate, CONTRACT_NAME, CONTRACT_VERSION},
    msg::{GroupContract, InstantiateMsg, MigrateMsg, QueryMsg},
    ContractError,
};

const DAO_ADDR: &str = "dao";
const ADDR1: &str = "addr1";
const ADDR2: &str = "addr2";
const ADDR3: &str = "addr3";
const ADDR4: &str = "addr4";

fn cw4_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cw4_group::contract::execute,
        cw4_group::contract::instantiate,
        cw4_group::contract::query,
    );
    Box::new(contract)
}

fn voting_contract() -> Box<dyn Contract<Empty>> {
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

fn instantiate_query_auth(app: &mut App) -> ContractInfo {
    let query_auth_info = app.store_code(query_auth_contract());
    let msg = shade_protocol::contract_interfaces::query_auth::InstantiateMsg {
        admin_auth: shade_protocol::Contract {
            address: Addr::unchecked("admin_contract"),
            code_hash: "code_hash".to_string(),
        },
        prng_seed: to_binary("seed").unwrap(),
    };

    app.instantiate_contract(
        query_auth_info,
        Addr::unchecked(DAO_ADDR),
        &msg,
        &[],
        "query_auth",
        None,
    )
    .unwrap()
}

fn create_viewing_key(app: &mut App, contract_info: ContractInfo, info: MessageInfo) -> String {
    let msg = shade_protocol::contract_interfaces::query_auth::ExecuteMsg::CreateViewingKey {
        entropy: "entropy".to_string(),
        padding: None,
    };
    let res = app
        .execute_contract(info.sender, &contract_info, &msg, &[])
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

fn instantiate_voting(
    app: &mut App,
    voting_info: ContractInstantiationInfo,
    msg: InstantiateMsg,
) -> ContractInfo {
    app.instantiate_contract(
        voting_info,
        Addr::unchecked(DAO_ADDR),
        &msg,
        &[],
        "voting module",
        None,
    )
    .unwrap()
}

fn setup_test_case(app: &mut App) -> (ContractInfo, ContractInfo) {
    let cw4_info = app.store_code(cw4_contract());
    let voting_info = app.store_code(voting_contract());

    let members = vec![
        cw4::Member {
            addr: ADDR1.to_string(),
            weight: 1,
        },
        cw4::Member {
            addr: ADDR2.to_string(),
            weight: 1,
        },
        cw4::Member {
            addr: ADDR3.to_string(),
            weight: 1,
        },
        cw4::Member {
            addr: ADDR4.to_string(),
            weight: 0,
        },
    ];

    let query_auth = instantiate_query_auth(app);

    (
        instantiate_voting(
            app,
            voting_info,
            InstantiateMsg {
                group_contract: GroupContract::New {
                    cw4_group_code_id: cw4_info.code_id,
                    cw4_group_code_hash: cw4_info.code_hash,
                    initial_members: members,
                    query_auth: Some(RawContract::new(
                        &query_auth.address.clone().to_string(),
                        &query_auth.code_hash.clone().to_string(),
                    )),
                },
                dao_code_hash: "todo!()".into(),
            },
        ),
        query_auth,
    )
}

#[test]
fn test_instantiate() {
    let mut app = App::default();
    // Valid instantiate no panics
    let _voting_contract_info = setup_test_case(&mut app);

    // Instantiate with no members, error
    let voting_info = app.store_code(voting_contract());
    let cw4_info = app.store_code(cw4_contract());
    let msg = InstantiateMsg {
        group_contract: GroupContract::New {
            cw4_group_code_id: cw4_info.code_id,
            cw4_group_code_hash: cw4_info.code_hash.clone(),
            initial_members: [].into(),
            query_auth: None,
        },
        dao_code_hash: "".to_string(),
    };
    let _err = app
        .instantiate_contract(
            voting_info.clone(),
            Addr::unchecked(DAO_ADDR),
            &msg,
            &[],
            "voting module",
            None,
        )
        .unwrap_err();

    // Instantiate with members but no weight
    let msg = InstantiateMsg {
        group_contract: GroupContract::New {
            cw4_group_code_id: cw4_info.code_id,
            cw4_group_code_hash: cw4_info.code_hash.clone(),
            initial_members: vec![
                cw4::Member {
                    addr: ADDR1.to_string(),
                    weight: 0,
                },
                cw4::Member {
                    addr: ADDR2.to_string(),
                    weight: 0,
                },
                cw4::Member {
                    addr: ADDR3.to_string(),
                    weight: 0,
                },
            ],
            query_auth: None,
        },
        dao_code_hash: "".to_string(),
    };
    let _err = app
        .instantiate_contract(
            voting_info,
            Addr::unchecked(DAO_ADDR),
            &msg,
            &[],
            "voting module",
            None,
        )
        .unwrap_err();
}

#[test]
pub fn test_instantiate_existing_contract() {
    let mut app = App::default();

    let voting_info = app.store_code(voting_contract());
    let cw4_info = app.store_code(cw4_contract());

    let query_auth = instantiate_query_auth(&mut app);

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.address.clone(),
            code_hash: query_auth.code_hash.clone(),
        },
        mock_info(ADDR1, &[]),
    );

    // Fail with no members.
    let cw4_contract_info = app
        .instantiate_contract(
            cw4_info.clone(),
            Addr::unchecked(DAO_ADDR),
            &cw4_group::msg::InstantiateMsg {
                admin: Some(DAO_ADDR.to_string()),
                members: vec![],
                query_auth: RawContract::new(
                    &query_auth.address.clone().to_string(),
                    &query_auth.code_hash.clone().to_string(),
                ),
            },
            &[],
            "cw4 group",
            None,
        )
        .unwrap();

    let err: ContractError = app
        .instantiate_contract(
            voting_info.clone(),
            Addr::unchecked(DAO_ADDR),
            &InstantiateMsg {
                group_contract: GroupContract::Existing {
                    address: cw4_contract_info.address.clone().to_string(),
                    code_hash: cw4_contract_info.code_hash.clone(),
                },
                dao_code_hash: "todo!()".into(),
            },
            &[],
            "voting module",
            None,
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    assert_eq!(err, ContractError::NoMembers {});

    let cw4_contract_info = app
        .instantiate_contract(
            cw4_info,
            Addr::unchecked(DAO_ADDR),
            &cw4_group::msg::InstantiateMsg {
                admin: Some(DAO_ADDR.to_string()),
                members: vec![cw4::Member {
                    addr: ADDR1.to_string(),
                    weight: 1,
                }],
                query_auth: RawContract::new(
                    &query_auth.address.clone().to_string(),
                    &query_auth.code_hash.clone().to_string(),
                ),
            },
            &[],
            "cw4 group",
            None,
        )
        .unwrap();

    // Instantiate with existing contract
    let msg = InstantiateMsg {
        group_contract: GroupContract::Existing {
            address: cw4_contract_info.address.clone().to_string(),
            code_hash: cw4_contract_info.code_hash.clone(),
        },
        dao_code_hash: "".into(),
    };
    let _err = app
        .instantiate_contract(
            voting_info,
            Addr::unchecked(DAO_ADDR),
            &msg,
            &[],
            "voting module",
            None,
        )
        .unwrap();

    // Update ADDR1's weight to 2
    let msg = cw4_group::msg::ExecuteMsg::UpdateMembers {
        remove: vec![],
        add: vec![cw4::Member {
            addr: ADDR1.to_string(),
            weight: 2,
        }],
    };

    app.execute_contract(
        Addr::unchecked(DAO_ADDR),
        &cw4_contract_info.clone(),
        &msg,
        &[],
    )
    .unwrap();

    // Same should be true about the groups contract.
    let cw4_power: cw4::MemberResponse = app
        .wrap()
        .query_wasm_smart(
            cw4_contract_info.code_hash.clone(),
            cw4_contract_info.address.clone(),
            &cw4::Cw4QueryMsg::Member {
                at_height: None,
                auth: Auth::ViewingKey {
                    key: viewing_key.into(),
                    address: ADDR1.into(),
                },
            },
        )
        .unwrap();
    assert_eq!(cw4_power.weight.unwrap(), 2);
}

#[test]
fn test_contract_info() {
    let mut app = App::default();
    let (voting_contract_info, _) = setup_test_case(&mut app);

    let info: InfoResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::Info {},
        )
        .unwrap();
    assert_eq!(
        info,
        InfoResponse {
            info: ContractVersion {
                contract: "crates.io:dao-voting-cw4".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string()
            }
        }
    );

    // Ensure group contract is set
    let _group_contract: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::GroupContract {},
        )
        .unwrap();

    let dao_contract: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash,
            voting_contract_info.address,
            &QueryMsg::Dao {},
        )
        .unwrap();
    assert_eq!(
        dao_contract,
        AnyContractInfo {
            addr: Addr::unchecked(DAO_ADDR),
            code_hash: "todo!()".into(),
        }
    );
}

#[test]
fn test_power_at_height() {
    let mut app = App::default();
    let (voting_contract_info, query_auth) = setup_test_case(&mut app);

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.address.clone(),
            code_hash: query_auth.code_hash.clone(),
        },
        mock_info(ADDR1, &[]),
    );

    let viewing_key_addr2 = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.address.clone(),
            code_hash: query_auth.code_hash.clone(),
        },
        mock_info(ADDR2, &[]),
    );

    app.update_block(next_block);

    let cw4_contract_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::GroupContract {},
        )
        .unwrap();

    let addr1_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: ADDR1.to_string(),
                },
                height: None,
            },
        )
        .unwrap();
    assert_eq!(addr1_voting_power.power, Uint128::new(1u128));
    assert_eq!(addr1_voting_power.height, app.block_info().height);

    let total_voting_power: TotalPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::TotalPowerAtHeight { height: None },
        )
        .unwrap();
    assert_eq!(total_voting_power.power, Uint128::new(3u128));
    assert_eq!(total_voting_power.height, app.block_info().height);

    // Update ADDR1's weight to 2
    let msg = cw4_group::msg::ExecuteMsg::UpdateMembers {
        remove: vec![],
        add: vec![cw4::Member {
            addr: ADDR1.to_string(),
            weight: 2,
        }],
    };

    // Should still be one as voting power should not update until
    // the following block.
    let addr1_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: ADDR1.to_string(),
                },
                height: None,
            },
        )
        .unwrap();
    assert_eq!(addr1_voting_power.power, Uint128::new(1u128));

    // Same should be true about the groups contract.
    let cw4_power: cw4::MemberResponse = app
        .wrap()
        .query_wasm_smart(
            cw4_contract_info.code_hash.clone(),
            cw4_contract_info.addr.clone(),
            &cw4::Cw4QueryMsg::Member {
                auth: Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: ADDR1.to_string(),
                },
                at_height: None,
            },
        )
        .unwrap();
    assert_eq!(cw4_power.weight.unwrap(), 1);

    app.execute_contract(
        Addr::unchecked(DAO_ADDR),
        &ContractInfo {
            address: cw4_contract_info.addr.clone(),
            code_hash: cw4_contract_info.code_hash.clone(),
        },
        &msg,
        &[],
    )
    .unwrap();
    app.update_block(next_block);

    // Should now be 2
    let addr1_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: ADDR1.to_string(),
                },
                height: None,
            },
        )
        .unwrap();
    assert_eq!(addr1_voting_power.power, Uint128::new(2u128));
    assert_eq!(addr1_voting_power.height, app.block_info().height);

    // Check we can still get the 1 weight he had last block
    let addr1_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: ADDR1.to_string(),
                },
                height: Some(app.block_info().height - 1),
            },
        )
        .unwrap();
    assert_eq!(addr1_voting_power.power, Uint128::new(1u128));
    assert_eq!(addr1_voting_power.height, app.block_info().height - 1);

    // Check total power is now 4
    let total_voting_power: TotalPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::TotalPowerAtHeight { height: None },
        )
        .unwrap();
    assert_eq!(total_voting_power.power, Uint128::new(4u128));
    assert_eq!(total_voting_power.height, app.block_info().height);

    // Check total power for last block is 3
    let total_voting_power: TotalPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::TotalPowerAtHeight {
                height: Some(app.block_info().height - 1),
            },
        )
        .unwrap();
    assert_eq!(total_voting_power.power, Uint128::new(3u128));
    assert_eq!(total_voting_power.height, app.block_info().height - 1);

    // Update ADDR1's weight back to 1
    let msg = cw4_group::msg::ExecuteMsg::UpdateMembers {
        remove: vec![],
        add: vec![cw4::Member {
            addr: ADDR1.to_string(),
            weight: 1,
        }],
    };

    app.execute_contract(
        Addr::unchecked(DAO_ADDR),
        &ContractInfo {
            address: cw4_contract_info.addr.clone(),
            code_hash: cw4_contract_info.code_hash.clone(),
        },
        &msg,
        &[],
    )
    .unwrap();
    app.update_block(next_block);

    // Should now be 1 again
    let addr1_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: ADDR1.to_string(),
                },
                height: None,
            },
        )
        .unwrap();
    assert_eq!(addr1_voting_power.power, Uint128::new(1u128));
    assert_eq!(addr1_voting_power.height, app.block_info().height);

    // Check total power for current block is now 3
    let total_voting_power: TotalPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::TotalPowerAtHeight { height: None },
        )
        .unwrap();
    assert_eq!(total_voting_power.power, Uint128::new(3u128));
    assert_eq!(total_voting_power.height, app.block_info().height);

    // Check total power for last block is 4
    let total_voting_power: TotalPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::TotalPowerAtHeight {
                height: Some(app.block_info().height - 1),
            },
        )
        .unwrap();
    assert_eq!(total_voting_power.power, Uint128::new(4u128));
    assert_eq!(total_voting_power.height, app.block_info().height - 1);

    // Remove address 2 completely
    let msg = cw4_group::msg::ExecuteMsg::UpdateMembers {
        remove: vec![ADDR2.to_string()],
        add: vec![],
    };

    app.execute_contract(
        Addr::unchecked(DAO_ADDR),
        &ContractInfo {
            address: cw4_contract_info.addr.clone(),
            code_hash: cw4_contract_info.code_hash.clone(),
        },
        &msg,
        &[],
    )
    .unwrap();
    app.update_block(next_block);

    // ADDR2 power is now 0
    let addr2_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key_addr2.clone(),
                    address: ADDR2.to_string(),
                },
                height: None,
            },
        )
        .unwrap();
    assert_eq!(addr2_voting_power.power, Uint128::zero());
    assert_eq!(addr2_voting_power.height, app.block_info().height);

    // Check total power for current block is now 2
    let total_voting_power: TotalPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::TotalPowerAtHeight { height: None },
        )
        .unwrap();
    assert_eq!(total_voting_power.power, Uint128::new(2u128));
    assert_eq!(total_voting_power.height, app.block_info().height);

    // Check total power for last block is 3
    let total_voting_power: TotalPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::TotalPowerAtHeight {
                height: Some(app.block_info().height - 1),
            },
        )
        .unwrap();
    assert_eq!(total_voting_power.power, Uint128::new(3u128));
    assert_eq!(total_voting_power.height, app.block_info().height - 1);

    // Readd ADDR2 with 10 power
    let msg = cw4_group::msg::ExecuteMsg::UpdateMembers {
        remove: vec![],
        add: vec![cw4::Member {
            addr: ADDR2.to_string(),
            weight: 10,
        }],
    };

    app.execute_contract(
        Addr::unchecked(DAO_ADDR),
        &ContractInfo {
            address: cw4_contract_info.addr.clone(),
            code_hash: cw4_contract_info.code_hash.clone(),
        },
        &msg,
        &[],
    )
    .unwrap();
    app.update_block(next_block);

    // ADDR2 power is now 10
    let addr2_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key_addr2.clone(),
                    address: ADDR2.to_string(),
                },
                height: None,
            },
        )
        .unwrap();
    assert_eq!(addr2_voting_power.power, Uint128::new(10u128));
    assert_eq!(addr2_voting_power.height, app.block_info().height);

    // Check total power for current block is now 12
    let total_voting_power: TotalPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::TotalPowerAtHeight { height: None },
        )
        .unwrap();
    assert_eq!(total_voting_power.power, Uint128::new(12u128));
    assert_eq!(total_voting_power.height, app.block_info().height);

    // Check total power for last block is 2
    let total_voting_power: TotalPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash,
            voting_contract_info.address,
            &QueryMsg::TotalPowerAtHeight {
                height: Some(app.block_info().height - 1),
            },
        )
        .unwrap();
    assert_eq!(total_voting_power.power, Uint128::new(2u128));
    assert_eq!(total_voting_power.height, app.block_info().height - 1);
}

#[test]
fn test_migrate() {
    let mut app = App::default();

    let initial_members = vec![
        cw4::Member {
            addr: ADDR1.to_string(),
            weight: 1,
        },
        cw4::Member {
            addr: ADDR2.to_string(),
            weight: 1,
        },
        cw4::Member {
            addr: ADDR3.to_string(),
            weight: 1,
        },
    ];

    let query_auth = instantiate_query_auth(&mut app);

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.address.clone(),
            code_hash: query_auth.code_hash.clone(),
        },
        mock_info(ADDR1, &[]),
    );

    // Instantiate with no members, error
    let voting_info = app.store_code(voting_contract());
    let cw4_info = app.store_code(cw4_contract());
    let msg = InstantiateMsg {
        group_contract: GroupContract::New {
            cw4_group_code_id: cw4_info.code_id,
            cw4_group_code_hash: cw4_info.code_hash,
            initial_members,
            query_auth: Some(RawContract::new(
                &query_auth.address.to_string(),
                &query_auth.code_hash.to_string(),
            )),
        },
        dao_code_hash: "todo!()".into(),
    };
    let voting_contract_info = app
        .instantiate_contract(
            voting_info.clone(),
            Addr::unchecked(DAO_ADDR),
            &msg,
            &[],
            "voting module",
            Some(DAO_ADDR.to_string()),
        )
        .unwrap();

    let power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: ADDR1.into(),
                },
                height: None,
            },
        )
        .unwrap();

    app.execute(
        Addr::unchecked(DAO_ADDR),
        CosmosMsg::Wasm(WasmMsg::Migrate {
            contract_addr: voting_contract_info.address.clone().to_string(),
            code_id: voting_info.code_id,
            code_hash: voting_contract_info.code_hash.clone(),
            msg: to_binary(&MigrateMsg {}).unwrap(),
        }),
    )
    .unwrap();

    let new_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash,
            voting_contract_info.address,
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key,
                    address: ADDR1.into(),
                },
                height: None,
            },
        )
        .unwrap();

    assert_eq!(new_power, power)
}

// #[test]
// fn test_duplicate_member() {
//     let mut app = App::default();
//     let _voting_addr = setup_test_case(&mut app);
//     let voting_info = app.store_code(voting_contract());
//     let cw4_info = app.store_code(cw4_contract());
//     // Instantiate with members but have a duplicate
//     // Total weight is actually 69 but ADDR3 appears twice.
//     let msg = InstantiateMsg {
//         group_contract: GroupContract::New {
//             cw4_group_code_id: cw4_info,
//             initial_members: vec![
//                 cw4::Member {
//                     addr: ADDR3.to_string(), // same address above
//                     weight: 19,
//                 },
//                 cw4::Member {
//                     addr: ADDR1.to_string(),
//                     weight: 25,
//                 },
//                 cw4::Member {
//                     addr: ADDR2.to_string(),
//                     weight: 25,
//                 },
//                 cw4::Member {
//                     addr: ADDR3.to_string(),
//                     weight: 19,
//                 },
//             ],
//         },
//     };
//     // Previous versions voting power was 100, due to no dedup.
//     // Now we error
//     // Bug busted : )
//     let _voting_addr = app
//         .instantiate_contract(
//             voting_info,
//             Addr::unchecked(DAO_ADDR),
//             &msg,
//             &[],
//             "voting module",
//             None,
//         )
//         .unwrap_err();
// }

// #[test]
// fn test_zero_voting_power() {
//     let mut app = App::default();
//     let voting_contract_info = setup_test_case(&mut app);
//     app.update_block(next_block);

//     let cw4_contract_info: Addr = app
//         .wrap()
//         .query_wasm_smart(voting_contract_info.clone(), &QueryMsg::GroupContract {})
//         .unwrap();

//     // check that ADDR4 weight is 0
//     let addr4_voting_power: VotingPowerAtHeightResponse = app
//         .wrap()
//         .query_wasm_smart(
//             voting_contract_info.clone(),
//             &QueryMsg::VotingPowerAtHeight {
//                 address: ADDR4.to_string(),
//                 height: None,
//             },
//         )
//         .unwrap();
//     assert_eq!(addr4_voting_power.power, Uint128::new(0));
//     assert_eq!(addr4_voting_power.height, app.block_info().height);

//     // Update ADDR1's weight to 0
//     let msg = cw4_group::msg::ExecuteMsg::UpdateMembers {
//         remove: vec![],
//         add: vec![cw4::Member {
//             addr: ADDR1.to_string(),
//             weight: 0,
//         }],
//     };
//     app.execute_contract(Addr::unchecked(DAO_ADDR), cw4_contract_info, &msg, &[])
//         .unwrap();

//     // Check ADDR1's power is now 0
//     let addr1_voting_power: VotingPowerAtHeightResponse = app
//         .wrap()
//         .query_wasm_smart(
//             voting_contract_info.clone(),
//             &QueryMsg::VotingPowerAtHeight {
//                 address: ADDR1.to_string(),
//                 height: None,
//             },
//         )
//         .unwrap();
//     assert_eq!(addr1_voting_power.power, Uint128::new(0u128));
//     assert_eq!(addr1_voting_power.height, app.block_info().height);

//     // Check total power is now 2
//     let total_voting_power: TotalPowerAtHeightResponse = app
//         .wrap()
//         .query_wasm_smart(voting_contract_info, &QueryMsg::TotalPowerAtHeight { height: None })
//         .unwrap();
//     assert_eq!(total_voting_power.power, Uint128::new(2u128));
//     assert_eq!(total_voting_power.height, app.block_info().height);
// }

// #[test]
// pub fn test_migrate_update_version() {
//     let mut deps = mock_dependencies();
//     cw2::set_contract_version(&mut deps.storage, "my-contract", "1.0.0").unwrap();
//     migrate(deps.as_mut(), mock_env(), MigrateMsg {}).unwrap();
//     let version = cw2::get_contract_version(&deps.storage).unwrap();
//     assert_eq!(version.version, CONTRACT_VERSION);
//     assert_eq!(version.contract, CONTRACT_NAME);
// }
