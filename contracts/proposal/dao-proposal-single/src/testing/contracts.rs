use cosmwasm_std::Empty;

use dao_pre_propose_single as cppbps;
use secret_multi_test::{Contract, ContractWrapper};

pub(crate) fn snip20_base_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip20_reference_impl::contract::execute,
        snip20_reference_impl::contract::instantiate,
        snip20_reference_impl::contract::query,
    );
    Box::new(contract)
}

pub(crate) fn cw4_group_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cw4_group::contract::execute,
        cw4_group::contract::instantiate,
        cw4_group::contract::query,
    );
    Box::new(contract)
}

pub(crate) fn snip721_base_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip721_reference_impl::contract::execute,
        snip721_reference_impl::contract::instantiate,
        snip721_reference_impl::contract::query,
    );
    Box::new(contract)
}

pub(crate) fn snip20_stake_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip20_stake::contract::execute,
        snip20_stake::contract::instantiate,
        snip20_stake::contract::query,
    );
    Box::new(contract)
}

pub(crate) fn proposal_single_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_reply(crate::contract::reply)
    .with_migrate(crate::contract::migrate);
    Box::new(contract)
}

pub(crate) fn pre_propose_single_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cppbps::contract::execute,
        cppbps::contract::instantiate,
        cppbps::contract::query,
    );
    Box::new(contract)
}

pub(crate) fn snip20_staked_balances_voting_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_voting_snip20_staked::contract::execute,
        dao_voting_snip20_staked::contract::instantiate,
        dao_voting_snip20_staked::contract::query,
    )
    .with_reply(dao_voting_snip20_staked::contract::reply);
    Box::new(contract)
}

pub(crate) fn native_staked_balances_voting_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_voting_token_staked::contract::execute,
        dao_voting_token_staked::contract::instantiate,
        dao_voting_token_staked::contract::query,
    );
    Box::new(contract)
}

pub(crate) fn snip721_stake_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_voting_snip721_staked::contract::execute,
        dao_voting_snip721_staked::contract::instantiate,
        dao_voting_snip721_staked::contract::query,
    );
    Box::new(contract)
}

pub(crate) fn cw_core_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_dao_core::contract::execute,
        dao_dao_core::contract::instantiate,
        dao_dao_core::contract::query,
    )
    .with_reply(dao_dao_core::contract::reply);
    Box::new(contract)
}

pub(crate) fn cw4_voting_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_voting_cw4::contract::execute,
        dao_voting_cw4::contract::instantiate,
        dao_voting_cw4::contract::query,
    )
    .with_reply(dao_voting_cw4::contract::reply);
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
