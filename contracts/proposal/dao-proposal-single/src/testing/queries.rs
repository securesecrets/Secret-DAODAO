use cosmwasm_std::{Addr, ContractInfo, Uint128};
use dao_interface::state::{
    AnyContractInfo, ProposalModule, ProposalModuleStatus, VotingModuleInfo,
};
use secret_multi_test::App;

use cw_hooks::HooksResponse;
use dao_pre_propose_single as cppbps;
use dao_voting::pre_propose::ProposalCreationPolicy;
use shade_protocol::basic_staking::Auth;

use crate::{
    msg::QueryMsg,
    query::{ProposalListResponse, ProposalResponse, VoteResponse},
    state::Config,
};

pub(crate) fn query_deposit_config_and_pre_propose_module(
    app: &App,
    proposal_single_addr: &Addr,
    proposal_single_code_hash: String,
) -> (cppbps::Config, ContractInfo) {
    let proposal_creation_policy =
        query_creation_policy(app, proposal_single_addr, proposal_single_code_hash);

    if let ProposalCreationPolicy::Module {
        addr: module_addr,
        code_hash,
    } = proposal_creation_policy
    {
        let deposit_config = query_pre_proposal_single_config(app, &module_addr, code_hash.clone());

        (
            deposit_config,
            ContractInfo {
                address: module_addr,
                code_hash,
            },
        )
    } else {
        panic!("no pre-propose module.")
    }
}

pub(crate) fn query_proposal_config(
    app: &App,
    proposal_single_addr: &Addr,
    proposal_single_code_hash: String,
) -> Config {
    app.wrap()
        .query_wasm_smart(
            proposal_single_code_hash,
            proposal_single_addr,
            &QueryMsg::Config {},
        )
        .unwrap()
}

pub(crate) fn query_creation_policy(
    app: &App,
    proposal_single_addr: &Addr,
    proposal_single_code_hash: String,
) -> ProposalCreationPolicy {
    app.wrap()
        .query_wasm_smart(
            proposal_single_code_hash,
            proposal_single_addr,
            &QueryMsg::ProposalCreationPolicy {},
        )
        .unwrap()
}

pub(crate) fn query_list_proposals(
    app: &App,
    proposal_single_addr: &Addr,
    proposal_single_code_hash: String,
    start_after: Option<u64>,
    limit: Option<u64>,
) -> ProposalListResponse {
    app.wrap()
        .query_wasm_smart(
            proposal_single_code_hash,
            proposal_single_addr,
            &QueryMsg::ListProposals { start_after, limit },
        )
        .unwrap()
}

pub(crate) fn query_vote(
    app: &App,
    proposal_module_addr: &Addr,
    proposal_module_code_hash: String,
    auth: Auth,
    proposal_id: u64,
) -> VoteResponse {
    app.wrap()
        .query_wasm_smart(
            proposal_module_code_hash,
            proposal_module_addr,
            &QueryMsg::GetVote {
                proposal_id,
                auth: Box::new(auth),
            },
        )
        .unwrap()
}

pub(crate) fn query_proposal_hooks(
    app: &App,
    proposal_single_addr: &Addr,
    proposal_single_code_hash: String,
) -> HooksResponse {
    app.wrap()
        .query_wasm_smart(
            proposal_single_code_hash,
            proposal_single_addr,
            &QueryMsg::ProposalHooks {},
        )
        .unwrap()
}

pub(crate) fn query_vote_hooks(
    app: &App,
    proposal_single_addr: &Addr,
    proposal_single_code_hash: String,
) -> HooksResponse {
    app.wrap()
        .query_wasm_smart(
            proposal_single_code_hash,
            proposal_single_addr,
            &QueryMsg::VoteHooks {},
        )
        .unwrap()
}

pub(crate) fn query_list_proposals_reverse(
    app: &App,
    proposal_single_addr: &Addr,
    proposal_single_code_hash: String,
    start_before: Option<u64>,
    limit: Option<u64>,
) -> ProposalListResponse {
    app.wrap()
        .query_wasm_smart(
            proposal_single_code_hash,
            proposal_single_addr,
            &QueryMsg::ReverseProposals {
                start_before,
                limit,
            },
        )
        .unwrap()
}

pub(crate) fn query_pre_proposal_single_config(
    app: &App,
    pre_propose_addr: &Addr,
    pre_propose_code_hash: String,
) -> cppbps::Config {
    app.wrap()
        .query_wasm_smart(
            pre_propose_code_hash,
            pre_propose_addr,
            &cppbps::QueryMsg::Config {},
        )
        .unwrap()
}

pub(crate) fn query_pre_proposal_single_deposit_info(
    app: &App,
    pre_propose_addr: &Addr,
    pre_propose_code_hash: String,
    proposal_id: u64,
) -> cppbps::DepositInfoResponse {
    app.wrap()
        .query_wasm_smart(
            pre_propose_code_hash,
            pre_propose_addr,
            &cppbps::QueryMsg::DepositInfo { proposal_id },
        )
        .unwrap()
}

pub(crate) fn query_single_proposal_module(
    app: &App,
    core_addr: &Addr,
    core_code_hash: String,
) -> AnyContractInfo {
    let modules: Vec<ProposalModule> = app
        .wrap()
        .query_wasm_smart(
            core_code_hash,
            core_addr,
            &dao_interface::msg::QueryMsg::ProposalModules {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    // Filter out disabled modules.
    let modules = modules
        .into_iter()
        .filter(|module| module.status == ProposalModuleStatus::Enabled)
        .collect::<Vec<_>>();

    assert_eq!(
        modules.len(),
        1,
        "wrong proposal module count. expected 1, got {}",
        modules.len()
    );

    AnyContractInfo {
        addr: modules.clone().into_iter().next().unwrap().address,
        code_hash: modules.into_iter().next().unwrap().code_hash,
    }
}

pub(crate) fn query_dao_token(
    app: &App,
    core_addr: &Addr,
    core_code_hash: String,
) -> AnyContractInfo {
    let voting_module = query_voting_module(app, core_addr, core_code_hash);
    app.wrap()
        .query_wasm_smart(
            voting_module.code_hash,
            voting_module.addr,
            &dao_interface::voting::Query::TokenContract {},
        )
        .unwrap()
}

pub(crate) fn query_voting_module(
    app: &App,
    core_addr: &Addr,
    core_code_hash: String,
) -> VotingModuleInfo {
    app.wrap()
        .query_wasm_smart(
            core_code_hash,
            core_addr,
            &dao_interface::msg::QueryMsg::VotingModule {},
        )
        .unwrap()
}

pub(crate) fn query_balance_cw20<
    T: Into<String>,
    U: Into<String>,
    K: Into<String>,
    C: Into<String>,
>(
    app: &App,
    contract_addr: T,
    code_hash: C,
    address: U,
    key: K,
) -> Uint128 {
    let msg = snip20_base::msg::QueryMsg::Balance {
        address: address.into(),
        key: key.into(),
    };
    let mut balance_amount = Uint128::zero();
    let result: snip20_base::msg::QueryAnswer = app
        .wrap()
        .query_wasm_smart(code_hash, contract_addr, &msg)
        .unwrap();
    match result {
        snip20_base::msg::QueryAnswer::Balance { amount } => {
            balance_amount = amount;
        }
        _ => (),
    }
    balance_amount
}

pub(crate) fn query_balance_native(app: &App, who: &str, denom: &str) -> Uint128 {
    let res = app.wrap().query_balance(who, denom).unwrap();
    res.amount
}

pub(crate) fn query_proposal(
    app: &App,
    proposal_single_addr: &Addr,
    proposal_single_code_hash: String,
    id: u64,
) -> ProposalResponse {
    app.wrap()
        .query_wasm_smart(
            proposal_single_code_hash,
            proposal_single_addr,
            &QueryMsg::Proposal { proposal_id: id },
        )
        .unwrap()
}

pub(crate) fn query_next_proposal_id(
    app: &App,
    proposal_single_addr: &Addr,
    proposal_single_code_hash: String,
) -> u64 {
    app.wrap()
        .query_wasm_smart(
            proposal_single_code_hash,
            proposal_single_addr,
            &QueryMsg::NextProposalId {},
        )
        .unwrap()
}
