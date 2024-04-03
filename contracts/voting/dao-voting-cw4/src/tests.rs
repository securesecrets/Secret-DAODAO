use cosmwasm_std::{from_binary, to_binary, Addr, ContractInfo, Empty, MessageInfo};
use dao_interface::{state::AnyContractInfo, voting::InfoResponse};
use secret_cw2::ContractVersion;
use secret_multi_test::{App, Contract, ContractInstantiationInfo, ContractWrapper, Executor};
use shade_protocol::utils::asset::RawContract;

use crate::msg::{GroupContract, InstantiateMsg, QueryMsg};

const DAO_ADDR: &str = "dao";
const ADDR1: &str = "addr1";
const ADDR2: &str = "addr2";
const ADDR3: &str = "addr3";
#[allow(dead_code)]
const ADDR4: &str = "addr4";
const OWNER: &str = "owner";

fn cw4_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cw4_group::contract::execute,
        cw4_group::contract::instantiate,
        cw4_group::contract::query,
    );
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

#[allow(dead_code)]
fn instantiate_voting(
    app: &mut App,
    contract_instantiate_info: ContractInstantiationInfo,
    msg: InstantiateMsg,
) -> ContractInfo {
    app.instantiate_contract(
        contract_instantiate_info,
        Addr::unchecked(DAO_ADDR),
        &msg,
        &[],
        "voting module",
        None,
    )
    .unwrap()
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
        Addr::unchecked(OWNER),
        &msg,
        &[],
        "query_auth",
        None,
    )
    .unwrap()
}

fn _create_viewing_key(app: &mut App, contract_info: ContractInfo, info: MessageInfo) -> String {
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

fn _setup_test_case(app: &mut App) -> ContractInfo {
    let cw4_instantiate_info = app.store_code(cw4_contract());
    let voting_instantiate_info = app.store_code(voting_contract());

    let query_auth = instantiate_query_auth(app);

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
    instantiate_voting(
        app,
        voting_instantiate_info,
        InstantiateMsg {
            group_contract: GroupContract::New {
                cw4_group_code_id: cw4_instantiate_info.code_id,
                cw4_group_code_hash: cw4_instantiate_info.code_hash,
                initial_members: members,
                query_auth: RawContract {
                    address: query_auth.address.to_string(),
                    code_hash: query_auth.code_hash,
                },
            },
            dao_code_hash: "dao_code_hash".to_string(),
        },
    )
}

#[test]
fn test_instantiate() {
    let mut app = App::default();

    // Instantiate with no members, error
    let voting_instantiate_info = app.store_code(voting_contract());
    let cw4_instantiate_info = app.store_code(cw4_contract());

    let query_auth = instantiate_query_auth(&mut app);

    let msg = InstantiateMsg {
        group_contract: GroupContract::New {
            cw4_group_code_id: cw4_instantiate_info.clone().code_id,
            cw4_group_code_hash: cw4_instantiate_info.clone().code_hash,
            initial_members: [].into(),
            query_auth: RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            },
        },
        dao_code_hash: "dao_code_Hash".to_string(),
    };
    let _err = app
        .instantiate_contract(
            voting_instantiate_info.clone(),
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
            cw4_group_code_id: cw4_instantiate_info.clone().code_id,
            cw4_group_code_hash: cw4_instantiate_info.clone().code_hash,
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
            query_auth: RawContract {
                address: query_auth.address.to_string(),
                code_hash: query_auth.code_hash,
            },
        },
        dao_code_hash: "dao_code_hash".to_string(),
    };
    let _err = app
        .instantiate_contract(
            voting_instantiate_info,
            Addr::unchecked(DAO_ADDR),
            &msg,
            &[],
            "voting module",
            None,
        )
        .unwrap_err();
}

#[test]
fn test_contract_info() {
    let mut app = App::default();

    let voting_instantiate_info = app.store_code(voting_contract());
    let cw4_instantiate_info = app.store_code(cw4_contract());
    let query_auth = instantiate_query_auth(&mut app);

    let cw4_info_with_member = app
        .instantiate_contract(
            cw4_instantiate_info.clone(),
            Addr::unchecked(DAO_ADDR),
            &cw4_group::msg::InstantiateMsg {
                admin: Some(DAO_ADDR.to_string()),
                members: vec![
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
                query_auth: RawContract {
                    address: query_auth.clone().address.to_string(),
                    code_hash: query_auth.clone().code_hash,
                },
            },
            &[],
            "cw4 group",
            None,
        )
        .unwrap();

    // Instantiate with existing contract
    let msg = InstantiateMsg {
        group_contract: GroupContract::Existing {
            address: cw4_info_with_member.clone().address.to_string(),
            code_hash: cw4_info_with_member.clone().code_hash,
        },
        dao_code_hash: "dao_code_hash".to_string(),
    };
    let voting_info = app
        .instantiate_contract(
            voting_instantiate_info.clone(),
            Addr::unchecked(DAO_ADDR),
            &msg,
            &[],
            "voting module",
            None,
        )
        .unwrap();

    let info: InfoResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
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
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::GroupContract {},
        )
        .unwrap();

    let dao_contract: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_info.code_hash,
            voting_info.address.to_string(),
            &QueryMsg::Dao {},
        )
        .unwrap();
    assert_eq!(
        dao_contract,
        AnyContractInfo {
            addr: Addr::unchecked(DAO_ADDR),
            code_hash: "dao_code_hash".to_string()
        }
    );
}
