use cosmwasm_std::Empty;
use dao_pre_propose_multiple as cppm;
use dao_pre_propose_single as cpps;
use secret_multi_test::{Contract, ContractWrapper};

pub fn snip20_base_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip20_reference_impl::contract::execute,
        snip20_reference_impl::contract::instantiate,
        snip20_reference_impl::contract::query,
    );
    Box::new(contract)
}

pub fn cw4_group_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cw4_group::contract::execute,
        cw4_group::contract::instantiate,
        cw4_group::contract::query,
    );
    Box::new(contract)
}

pub fn snip721_base_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip721_reference_impl::contract::execute,
        snip721_reference_impl::contract::instantiate,
        snip721_reference_impl::contract::query,
    );
    Box::new(contract)
}

pub fn snip721_roles_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip721_roles::contract::execute,
        snip721_roles::contract::instantiate,
        snip721_roles::contract::query,
    );
    Box::new(contract)
}

pub fn snip20_stake_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip20_stake::contract::execute,
        snip20_stake::contract::instantiate,
        snip20_stake::contract::query,
    );
    Box::new(contract)
}

pub fn proposal_condorcet_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_proposal_condorcet::contract::execute,
        dao_proposal_condorcet::contract::instantiate,
        dao_proposal_condorcet::contract::query,
    )
    .with_reply(dao_proposal_condorcet::contract::reply);
    Box::new(contract)
}

pub fn proposal_single_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_proposal_single::contract::execute,
        dao_proposal_single::contract::instantiate,
        dao_proposal_single::contract::query,
    )
    .with_reply(dao_proposal_single::contract::reply)
    .with_migrate(dao_proposal_single::contract::migrate);
    Box::new(contract)
}

pub fn pre_propose_single_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cpps::contract::execute,
        cpps::contract::instantiate,
        cpps::contract::query,
    );
    Box::new(contract)
}

pub fn pre_propose_multiple_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cppm::contract::execute,
        cppm::contract::instantiate,
        cppm::contract::query,
    );
    Box::new(contract)
}

pub fn snip20_staked_balances_voting_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_voting_snip20_staked::contract::execute,
        dao_voting_snip20_staked::contract::instantiate,
        dao_voting_snip20_staked::contract::query,
    )
    .with_reply(dao_voting_snip20_staked::contract::reply);
    Box::new(contract)
}

pub fn native_staked_balances_voting_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_voting_token_staked::contract::execute,
        dao_voting_token_staked::contract::instantiate,
        dao_voting_token_staked::contract::query,
    );
    Box::new(contract)
}

pub fn voting_snip721_staked_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_voting_snip721_staked::contract::execute,
        dao_voting_snip721_staked::contract::instantiate,
        dao_voting_snip721_staked::contract::query,
    )
    .with_reply(dao_voting_snip721_staked::contract::reply);
    Box::new(contract)
}

pub fn dao_dao_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_dao_core::contract::execute,
        dao_dao_core::contract::instantiate,
        dao_dao_core::contract::query,
    )
    .with_reply(dao_dao_core::contract::reply)
    .with_migrate(dao_dao_core::contract::migrate);
    Box::new(contract)
}

pub fn dao_voting_cw4_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_voting_cw4::contract::execute,
        dao_voting_cw4::contract::instantiate,
        dao_voting_cw4::contract::query,
    )
    .with_reply(dao_voting_cw4::contract::reply);
    Box::new(contract)
}

pub fn dao_voting_snip721_roles_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_voting_snip721_roles::contract::execute,
        dao_voting_snip721_roles::contract::instantiate,
        dao_voting_snip721_roles::contract::query,
    )
    .with_reply(dao_voting_snip721_roles::contract::reply);
    Box::new(contract)
}

pub fn cw_vesting_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cw_vesting::contract::execute,
        cw_vesting::contract::instantiate,
        cw_vesting::contract::query,
    );
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
