use cosmwasm_std::{
    from_binary, to_binary, Addr, Coin, ContractInfo, CosmosMsg, Decimal, MessageInfo, Uint128,
};
use secret_multi_test::{App, BankSudo, Executor};

use dao_voting::voting::Vote;
use secret_utils::Duration;
use shade_protocol::{basic_staking::Auth, utils::asset::RawContract};
use snip20_reference_impl::msg::InitialBalance;

use crate::{
    msg::{ExecuteMsg, QueryMsg},
    query::ProposalResponse,
    testing::queries::query_next_proposal_id,
    ContractError,
};

use super::{contracts::snip20_base_contract, CREATOR_ADDR};

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
    match auth.clone() {
        Auth::ViewingKey { address, .. } => {
            proposer = Addr::unchecked(address);
        }
        _ => (),
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
    match auth.clone() {
        Auth::ViewingKey { address, .. } => {
            sender = Addr::unchecked(address);
        }
        _ => (),
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
    match auth.clone() {
        Auth::ViewingKey { address, .. } => {
            sender = Addr::unchecked(address);
        }
        _ => (),
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
    match auth.clone() {
        Auth::ViewingKey { address, .. } => {
            sender = Addr::unchecked(address);
        }
        _ => (),
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
    match auth.clone() {
        Auth::ViewingKey { address, .. } => {
            sender = Addr::unchecked(address);
        }
        _ => (),
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
    match auth.clone() {
        Auth::ViewingKey { address, .. } => {
            sender = Addr::unchecked(address);
        }
        _ => (),
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
    query_auth: RawContract,
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
            dao: "dao_address".to_string(),
            code_hash: "dao_code_hash".to_string(),
            close_proposal_on_execution_failure: true,
            veto: None,
            query_auth,
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
    query_auth: RawContract,
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
            dao: "dao_address".to_string(),
            code_hash: "dao_code_hash".to_string(),
            close_proposal_on_execution_failure: true,
            veto: None,
            query_auth,
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

pub(crate) fn _mint_natives(app: &mut App, receiver: &str, amount: Vec<Coin>) {
    app.sudo(secret_multi_test::SudoMsg::Bank(BankSudo::Mint {
        to_address: receiver.to_string(),
        amount,
    }))
    .unwrap();
}

pub(crate) fn _mint_snip20s(
    app: &mut App,
    snip20_contract: &Addr,
    snip20_contract_code_hash: String,
    sender: &Addr,
    receiver: &str,
    amount: u128,
) {
    app.execute_contract(
        sender.clone(),
        &ContractInfo {
            address: snip20_contract.clone(),
            code_hash: snip20_contract_code_hash,
        },
        &snip20_reference_impl::msg::ExecuteMsg::Mint {
            recipient: receiver.to_string(),
            amount: Uint128::new(amount),
            memo: None,
            decoys: None,
            entropy: None,
            padding: None,
        },
        &[],
    )
    .unwrap();
}

pub(crate) fn _instantiate_snip20_base_default(
    app: &mut App,
    admin: Option<String>,
) -> ContractInfo {
    let snip20_contract_instantiate_info = app.store_code(snip20_base_contract());
    let snip20_instantiate = snip20_reference_impl::msg::InstantiateMsg {
        name: "snip20 token".to_string(),
        symbol: "cwtwenty".to_string(),
        decimals: 6,
        initial_balances: Some(vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(10_000_000),
        }]),
        admin,
        prng_seed: to_binary(&"prng_seed".to_string()).unwrap(),
        config: None,
        supported_denoms: None,
    };
    app.instantiate_contract(
        snip20_contract_instantiate_info,
        Addr::unchecked("ekez"),
        &snip20_instantiate,
        &[],
        "snip20-base",
        None,
    )
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
