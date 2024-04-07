use cosmwasm_std::{to_binary, Addr, ContractInfo, Empty};
use secret_multi_test::{App, Contract, ContractWrapper, Executor};
use shade_protocol::utils::asset::RawContract;

use crate::snip721roles;

pub(crate) const CREATOR_ADDR: &str = "creator";

fn snip721_roles_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip721_roles::contract::execute,
        snip721_roles::contract::instantiate,
        snip721_roles::contract::query,
    );
    Box::new(contract)
}

pub(crate) fn snip721_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip721_reference_impl::contract::execute,
        snip721_reference_impl::contract::instantiate,
        snip721_reference_impl::contract::query,
    );
    Box::new(contract)
}

pub(crate) fn query_auth_contract() -> Box<dyn Contract<Empty>> {
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
        Addr::unchecked(CREATOR_ADDR),
        &msg,
        &[],
        "query_auth",
        None,
    )
    .unwrap()
}

pub fn instantiate_snip721_roles(
    app: &mut App,
    sender: &str,
) -> ContractInfo {
    let snip721_roles_contract_instantiate_info = app.store_code(snip721_roles_contract());
    let snip721_contract_instantiate_info = app.store_code(snip721_contract());
    let query_auth = instantiate_query_auth(app);


    let snip721_roles_info = app
        .instantiate_contract(
            snip721_roles_contract_instantiate_info.clone(),
            Addr::unchecked(sender),
            &snip721roles::Snip721RolesInstantiateMsg {
                name: "bad kids".to_string(),
                symbol: "bad kids".to_string(),
                entropy: "entropy".to_string(),
                config: None,
                code_id: snip721_contract_instantiate_info.code_id,
                code_hash: snip721_contract_instantiate_info.code_hash,
                label: "snip721".to_string(),
                query_auth: RawContract{
                    address: query_auth.address.to_string(),
                    code_hash: query_auth.code_hash
                }
            },
            &[],
            "snip721_roles".to_string(),
            None,
        )
        .unwrap();

    snip721_roles_info
}
