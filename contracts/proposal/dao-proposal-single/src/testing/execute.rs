use cosmwasm_std::{from_binary, Addr, ContractInfo, CosmosMsg, Decimal, MessageInfo};
use secret_multi_test::{App, Executor};

use dao_voting::voting::Vote;
use secret_utils::Duration;
use shade_protocol::basic_staking::Auth;

use crate::{
    msg::{ExecuteMsg, QueryMsg},
    query::ProposalResponse,
    testing::queries::query_next_proposal_id,
    ContractError,
};

// Creates a proposal then checks that the proposal was created with
// the specified messages and returns the ID of the proposal.
//
// This expects that the proposer already has the needed tokens to pay
// the deposit.
pub(crate) fn make_proposal(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    auth: Auth,
    msgs: Vec<CosmosMsg>,
) {
    // let proposal_creation_policy =
    //     query_creation_policy(app, proposal_single, proposal_single_code_hash.clone());
    let mut proposer = Addr::unchecked("");
    if let Auth::ViewingKey { address, .. } = auth.clone() {
        proposer = Addr::unchecked(address);
    }

    app.execute_contract(
        Addr::unchecked(proposer.clone()),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash.clone(),
        },
        &ExecuteMsg::Propose(dao_voting::proposal::SingleChoiceProposeMsg {
            title: "title".to_string(),
            description: "description".to_string(),
            msgs: msgs.clone(),
            proposer: None,
        }),
        &[],
    )
    .unwrap();

    let id = query_next_proposal_id(app, proposal_single, proposal_single_code_hash.clone());
    let id = id - 1;

    // Check that the proposal was created as expected.
    let proposal: ProposalResponse = app
        .wrap()
        .query_wasm_smart(
            proposal_single_code_hash,
            proposal_single,
            &QueryMsg::Proposal { proposal_id: id },
        )
        .unwrap();

    assert_eq!(proposal.proposal.proposer, Addr::unchecked(proposer));
    assert_eq!(proposal.proposal.title, "title".to_string());
    assert_eq!(proposal.proposal.description, "description".to_string());
    assert_eq!(proposal.proposal.msgs, msgs);
}

pub(crate) fn _vote_on_proposal(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    auth: Auth,
    proposal_id: u64,
    vote: Vote,
) {
    let mut sender = Addr::unchecked("");
    if let Auth::ViewingKey { address, .. } = auth.clone() {
        sender = Addr::unchecked(address);
    }
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash.clone(),
        },
        &ExecuteMsg::Vote {
            auth,
            proposal_id,
            vote,
            rationale: None,
        },
        &[],
    )
    .unwrap();
}

pub(crate) fn vote_on_proposal_should_fail(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    auth: Auth,
    proposal_id: u64,
    vote: Vote,
) -> ContractError {
    let mut sender = Addr::unchecked("");
    if let Auth::ViewingKey { address, .. } = auth.clone() {
        sender = Addr::unchecked(address);
    }
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash,
        },
        &ExecuteMsg::Vote {
            auth,
            proposal_id,
            vote,
            rationale: None,
        },
        &[],
    )
    .unwrap_err()
    .downcast()
    .unwrap()
}

pub(crate) fn execute_proposal_should_fail(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    auth: Auth,
    proposal_id: u64,
) -> ContractError {
    let mut sender = Addr::unchecked("");
    if let Auth::ViewingKey { address, .. } = auth.clone() {
        sender = Addr::unchecked(address);
    }

    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash,
        },
        &ExecuteMsg::Execute { auth, proposal_id },
        &[],
    )
    .unwrap_err()
    .downcast()
    .unwrap()
}

pub(crate) fn _vote_on_proposal_with_rationale(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    auth: Auth,
    proposal_id: u64,
    vote: Vote,
    rationale: Option<String>,
) {
    let mut sender = Addr::unchecked("");
    if let Auth::ViewingKey { address, .. } = auth.clone() {
        sender = Addr::unchecked(address);
    }

    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash,
        },
        &ExecuteMsg::Vote {
            auth,
            proposal_id,
            vote,
            rationale,
        },
        &[],
    )
    .unwrap();
}

pub(crate) fn update_rationale(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    sender: &str,
    proposal_id: u64,
    rationale: Option<String>,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash,
        },
        &ExecuteMsg::UpdateRationale {
            proposal_id,
            rationale,
        },
        &[],
    )
    .unwrap();
}

pub(crate) fn _execute_proposal(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    auth: Auth,
    proposal_id: u64,
) {
    let mut sender = Addr::unchecked("");
    if let Auth::ViewingKey { address, .. } = auth.clone() {
        sender = Addr::unchecked(address);
    }

    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash,
        },
        &ExecuteMsg::Execute { auth, proposal_id },
        &[],
    )
    .unwrap();
}

pub(crate) fn close_proposal_should_fail(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    sender: &str,
    proposal_id: u64,
) -> ContractError {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash,
        },
        &ExecuteMsg::Close { proposal_id },
        &[],
    )
    .unwrap_err()
    .downcast()
    .unwrap()
}

pub(crate) fn _close_proposal(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    sender: &str,
    proposal_id: u64,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash,
        },
        &ExecuteMsg::Close { proposal_id },
        &[],
    )
    .unwrap();
}

pub(crate) fn update_config(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    sender: &str,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash,
        },
        &ExecuteMsg::UpdateConfig {
            threshold: dao_voting::threshold::Threshold::ThresholdQuorum {
                quorum: dao_voting::threshold::PercentageThreshold::Percent(Decimal::percent(15)),
                threshold: dao_voting::threshold::PercentageThreshold::Majority {},
            },
            max_voting_period: Duration::Time(604800), // One week.
            min_voting_period: None,
            only_members_execute: true,
            allow_revoting: false,
            close_proposal_on_execution_failure: true,
            veto: None,
        },
        &[],
    )
    .unwrap();
}

pub(crate) fn update_config_should_fail(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    sender: &str,
) -> ContractError {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash,
        },
        &ExecuteMsg::UpdateConfig {
            threshold: dao_voting::threshold::Threshold::ThresholdQuorum {
                quorum: dao_voting::threshold::PercentageThreshold::Percent(Decimal::percent(15)),
                threshold: dao_voting::threshold::PercentageThreshold::Majority {},
            },
            max_voting_period: Duration::Time(604800), // One week.
            min_voting_period: None,
            only_members_execute: true,
            allow_revoting: false,
            close_proposal_on_execution_failure: true,
            veto: None,
        },
        &[],
    )
    .unwrap_err()
    .downcast()
    .unwrap()
}

pub(crate) fn update_pre_propose_info(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    sender: &str,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash,
        },
        &ExecuteMsg::UpdatePreProposeInfo {
            info: dao_voting::pre_propose::PreProposeInfo::AnyoneMayPropose {},
        },
        &[],
    )
    .unwrap();
}

pub(crate) fn update_pre_propose_info_should_fail(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    sender: &str,
) -> ContractError {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash,
        },
        &ExecuteMsg::UpdatePreProposeInfo {
            info: dao_voting::pre_propose::PreProposeInfo::AnyoneMayPropose {},
        },
        &[],
    )
    .unwrap_err()
    .downcast()
    .unwrap()
}

pub(crate) fn execute_veto_fails(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    sender: &str,
    proposal_id: u64,
) -> ContractError {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash,
        },
        &ExecuteMsg::Veto { proposal_id },
        &[],
    )
    .unwrap_err()
    .downcast()
    .unwrap()
}

pub(crate) fn add_proposal_hook(
    app: &mut App,
    proposal_module: &Addr,
    proposal_module_code_hash: String,
    sender: &str,
    hook_addr: &str,
    hook_code_hash: &str,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash,
        },
        &ExecuteMsg::AddProposalHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash.to_string(),
        },
        &[],
    )
    .unwrap();
}

pub(crate) fn add_proposal_hook_should_fail(
    app: &mut App,
    proposal_module: &Addr,
    proposal_module_code_hash: String,
    sender: &str,
    hook_addr: &str,
    hook_code_hash: &str,
) -> ContractError {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash,
        },
        &ExecuteMsg::AddProposalHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash.to_string(),
        },
        &[],
    )
    .unwrap_err()
    .downcast()
    .unwrap()
}

pub(crate) fn remove_proposal_hook(
    app: &mut App,
    proposal_module: &Addr,
    proposal_module_code_hash: String,
    sender: &str,
    hook_addr: &str,
    hook_code_hash: &str,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash,
        },
        &ExecuteMsg::RemoveProposalHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash.to_string(),
        },
        &[],
    )
    .unwrap();
}

pub(crate) fn remove_proposal_hook_should_fail(
    app: &mut App,
    proposal_module: &Addr,
    proposal_module_code_hash: String,
    sender: &str,
    hook_addr: &str,
    hook_code_hash: &str,
) -> ContractError {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash,
        },
        &ExecuteMsg::RemoveProposalHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash.to_string(),
        },
        &[],
    )
    .unwrap_err()
    .downcast()
    .unwrap()
}

pub(crate) fn add_vote_hook(
    app: &mut App,
    proposal_module: &Addr,
    proposal_module_code_hash: String,
    sender: &str,
    hook_addr: &str,
    hook_code_hash: &str,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash,
        },
        &ExecuteMsg::AddVoteHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash.to_string(),
        },
        &[],
    )
    .unwrap();
}

pub(crate) fn add_vote_hook_should_fail(
    app: &mut App,
    proposal_module: &Addr,
    proposal_module_code_hash: String,
    sender: &str,
    hook_addr: &str,
    hook_code_hash: &str,
) -> ContractError {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash,
        },
        &ExecuteMsg::AddVoteHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash.to_string(),
        },
        &[],
    )
    .unwrap_err()
    .downcast()
    .unwrap()
}

pub(crate) fn remove_vote_hook(
    app: &mut App,
    proposal_module: &Addr,
    proposal_module_code_hash: String,
    sender: &str,
    hook_addr: &str,
    hook_code_hash: &str,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash,
        },
        &ExecuteMsg::RemoveVoteHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash.to_string(),
        },
        &[],
    )
    .unwrap();
}

pub(crate) fn remove_vote_hook_should_fail(
    app: &mut App,
    proposal_module: &Addr,
    proposal_module_code_hash: String,
    sender: &str,
    hook_addr: &str,
    hook_code_hash: &str,
) -> ContractError {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash,
        },
        &ExecuteMsg::RemoveVoteHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash.to_string(),
        },
        &[],
    )
    .unwrap_err()
    .downcast()
    .unwrap()
}

pub(crate) fn create_viewing_key(
    app: &mut App,
    contract_info: ContractInfo,
    info: MessageInfo,
) -> String {
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
