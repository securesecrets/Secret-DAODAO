use cosmwasm_std::{Addr, ContractInfo, Empty, Uint128};
use dao_interface::voting::InfoResponse;
use secret_cw2::ContractVersion;
use secret_multi_test::{App, Contract, ContractInstantiationInfo, ContractWrapper, Executor};
use snip20_reference_impl::msg::InitialBalance;

use crate::msg::{InstantiateMsg, QueryMsg};

const DAO_ADDR: &str = "dao";
const CREATOR_ADDR: &str = "creator";

fn snip20_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip20_reference_impl::contract::execute,
        snip20_reference_impl::contract::instantiate,
        snip20_reference_impl::contract::query,
    );
    Box::new(contract)
}

fn balance_voting_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_reply(crate::contract::reply);
    Box::new(contract)
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

#[test]
#[should_panic(expected = "Initial governance token balances must not be empty")]
fn test_instantiate_zero_supply() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(balance_voting_contract());
    instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                label: "DAO DAO voting".to_string(),
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::zero(),
                }],
            },
            dao_code_hash: "dao_code_hash".to_string(),
        },
    );
}

#[test]
#[should_panic(expected = "Initial governance token balances must not be empty")]
fn test_instantiate_no_balances() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(balance_voting_contract());
    instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                label: "DAO DAO voting".to_string(),
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![],
            },
            dao_code_hash: "dao_code_hash".to_string(),
        },
    );
}

#[test]
fn test_contract_info() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(balance_voting_contract());
    let voting_contract_info = instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                label: "DAO DAO voting".to_string(),
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::from(2u64),
                }],
            },
            dao_code_hash: "dao_code_hash".to_string(),
        },
    );

    let info: InfoResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash,
            voting_contract_info.address,
            &QueryMsg::Info {},
        )
        .unwrap();
    assert_eq!(
        info,
        InfoResponse {
            info: ContractVersion {
                contract: "crates.io:cw20-balance-voting".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string()
            }
        }
    )
}
