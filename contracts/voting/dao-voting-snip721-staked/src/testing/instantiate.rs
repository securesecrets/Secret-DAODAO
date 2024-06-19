use cosmwasm_std::{to_binary, Addr, ContractInfo, Empty};
use secret_multi_test::{App, Contract, ContractWrapper, Executor};
use snip721_reference_impl::msg::InstantiateConfig;

pub fn snip721_base_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip721_reference_impl::contract::execute,
        snip721_reference_impl::contract::instantiate,
        snip721_reference_impl::contract::query,
    );
    Box::new(contract)
}

pub fn voting_snip721_staked_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_reply(crate::contract::reply);
    Box::new(contract)
}

pub fn query_auth_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        query_auth::contract::execute,
        query_auth::contract::instantiate,
        query_auth::contract::query,
    );
    Box::new(contract)
}

pub fn instantiate_snip721_base(app: &mut App, sender: &str, minter: &str) -> ContractInfo {
    let snip721_contract_instantiation_info = app.store_code(snip721_base_contract());

    app.instantiate_contract(
        snip721_contract_instantiation_info,
        Addr::unchecked(sender),
        &snip721_reference_impl::msg::InstantiateMsg {
            name: "bad kids".to_string(),
            symbol: "bad kids".to_string(),
            admin: Some(minter.to_string()),
            entropy: "entropy".to_string(),
            royalty_info: None,
            config: Some(InstantiateConfig {
                public_token_supply: Some(true),
                public_owner: Some(true),
                enable_sealed_metadata: None,
                unwrapped_metadata_is_private: None,
                minter_may_update_metadata: None,
                owner_may_update_metadata: None,
                enable_burn: None,
            }),
            post_init_callback: None,
        },
        &[],
        "snip721_base".to_string(),
        None,
    )
    .unwrap()
}

pub fn instantiate_query_auth(app: &mut App, sender: &str) -> ContractInfo {
    let query_auth_contract_instantiation_info = app.store_code(query_auth_contract());

    app.instantiate_contract(
        query_auth_contract_instantiation_info,
        Addr::unchecked(sender),
        &shade_protocol::contract_interfaces::query_auth::InstantiateMsg {
            admin_auth: shade_protocol::Contract {
                address: Addr::unchecked(""),
                code_hash: "".to_string(),
            },
            prng_seed: to_binary(&"seed").unwrap(),
        },
        &[],
        "query_auth".to_string(),
        None,
    )
    .unwrap()
}
