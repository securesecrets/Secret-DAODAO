use super::{contracts::query_auth_contract, CREATOR_ADDR};
use crate::msg::InstantiateMsg;
use cosmwasm_std::{to_binary, Addr, ContractInfo};
use dao_voting::{pre_propose::PreProposeInfo, threshold::PercentageThreshold};
use secret_multi_test::{App, Executor};
use secret_utils::Duration;
use shade_protocol::utils::asset::RawContract;

pub(crate) fn get_default_token_dao_proposal_module_instantiate(
    query_auth: RawContract,
    dao_code_hash: String,
) -> InstantiateMsg {
    InstantiateMsg {
        veto: None,
        voting_strategy: dao_voting::multiple_choice::VotingStrategy::SingleChoice {
            quorum: PercentageThreshold::Majority {},
        },
        max_voting_period: Duration::Time(604800), // One week.
        min_voting_period: None,
        only_members_execute: true,
        allow_revoting: false,
        pre_propose_info: PreProposeInfo::AnyoneMayPropose {},
        close_proposal_on_execution_failure: true,
        dao_code_hash,
        query_auth: Some(query_auth),
    }
}

pub(crate) fn instantiate_query_auth(app: &mut App) -> ContractInfo {
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
