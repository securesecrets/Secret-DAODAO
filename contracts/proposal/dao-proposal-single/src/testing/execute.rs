use cosmwasm_std::{
    coins, from_binary, to_binary, Addr, Coin, ContractInfo, CosmosMsg, MessageInfo, Uint128,
};
use secret_multi_test::{App, BankSudo, Executor};

use cw_denom::CheckedDenom;
use dao_pre_propose_single as cppbps;
use dao_voting::{
    deposit::CheckedDepositInfo, pre_propose::ProposalCreationPolicy,
    proposal::SingleChoiceProposeMsg as ProposeMsg, voting::Vote,
};
use shade_protocol::basic_staking::Auth;
use snip20_base::msg::InitialBalance;

use crate::{
    msg::{ExecuteMsg, QueryMsg},
    query::ProposalResponse,
    testing::queries::{query_creation_policy, query_next_proposal_id},
    ContractError,
};

use super::{
    contracts::snip20_base_contract, queries::query_pre_proposal_single_config, CREATOR_ADDR,
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
    proposer: &str,
    auth: Auth,
    msgs: Vec<CosmosMsg>,
) -> u64 {
    let proposal_creation_policy =
        query_creation_policy(app, proposal_single, proposal_single_code_hash.clone());

    // Collect the funding.
    let funds = match proposal_creation_policy {
        ProposalCreationPolicy::Anyone {} => vec![],
        ProposalCreationPolicy::Module {
            addr: ref pre_propose,
            code_hash: ref pre_proposse_code_hash,
        } => {
            let deposit_config =
                query_pre_proposal_single_config(app, pre_propose, pre_proposse_code_hash.clone());
            match deposit_config.deposit_info {
                Some(CheckedDepositInfo {
                    denom,
                    amount,
                    refund_policy: _,
                }) => match denom {
                    CheckedDenom::Native(denom) => coins(amount.u128(), denom),
                    CheckedDenom::Snip20(addr, code_hash) => {
                        // Give an allowance, no funds.
                        app.execute_contract(
                            Addr::unchecked(proposer),
                            &ContractInfo {
                                address: addr,
                                code_hash,
                            },
                            &snip20_base::msg::ExecuteMsg::IncreaseAllowance {
                                spender: pre_propose.to_string(),
                                amount,
                                expiration: None,
                                padding: None,
                            },
                            &[],
                        )
                        .unwrap();
                        vec![]
                    }
                },
                None => vec![],
            }
        }
    };

    // Make the proposal.
    match proposal_creation_policy {
        ProposalCreationPolicy::Anyone {} => app
            .execute_contract(
                Addr::unchecked(proposer),
                &ContractInfo {
                    address: proposal_single.clone(),
                    code_hash: proposal_single_code_hash.clone(),
                },
                &ExecuteMsg::Propose(ProposeMsg {
                    title: "title".to_string(),
                    description: "description".to_string(),
                    msgs: msgs.clone(),
                    proposer: None,
                }),
                &[],
            )
            .unwrap(),
        ProposalCreationPolicy::Module { addr, code_hash } => app
            .execute_contract(
                Addr::unchecked(proposer),
                &ContractInfo {
                    address: addr,
                    code_hash,
                },
                &cppbps::ExecuteMsg::Propose {
                    msg: cppbps::ProposeMessage::Propose {
                        title: "title".to_string(),
                        description: "description".to_string(),
                        msgs: msgs.clone(),
                    },
                    auth,
                },
                &funds,
            )
            .unwrap(),
    };
    let id = query_next_proposal_id(app, proposal_single, proposal_single_code_hash.clone());
    let id = id - 1;

    // Check that the proposal was created as expected.
    let proposal: ProposalResponse = app
        .wrap()
        .query_wasm_smart(
            proposal_single_code_hash.clone(),
            proposal_single.clone(),
            &QueryMsg::Proposal { proposal_id: id },
        )
        .unwrap();

    assert_eq!(proposal.proposal.proposer, Addr::unchecked(proposer));
    assert_eq!(proposal.proposal.title, "title".to_string());
    assert_eq!(proposal.proposal.description, "description".to_string());
    assert_eq!(proposal.proposal.msgs, msgs);

    id
}

pub(crate) fn vote_on_proposal(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    sender: &str,
    proposal_id: u64,
    auth: Auth,
    vote: Vote,
) {
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
    sender: &str,
    auth: Auth,
    proposal_id: u64,
    vote: Vote,
) -> ContractError {
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
    .unwrap_err()
    .downcast()
    .unwrap()
}

pub(crate) fn execute_proposal_should_fail(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    sender: &str,
    auth: Auth,
    proposal_id: u64,
) -> ContractError {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash.clone(),
        },
        &ExecuteMsg::Execute { auth, proposal_id },
        &[],
    )
    .unwrap_err()
    .downcast()
    .unwrap()
}

pub(crate) fn vote_on_proposal_with_rationale(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    sender: &str,
    auth: Auth,
    proposal_id: u64,
    vote: Vote,
    rationale: Option<String>,
) {
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
            code_hash: proposal_single_code_hash.clone(),
        },
        &ExecuteMsg::UpdateRationale {
            proposal_id,
            rationale,
        },
        &[],
    )
    .unwrap();
}

pub(crate) fn execute_proposal(
    app: &mut App,
    proposal_single: &Addr,
    proposal_single_code_hash: String,
    sender: &str,
    auth: Auth,
    proposal_id: u64,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_single.clone(),
            code_hash: proposal_single_code_hash.clone(),
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
            code_hash: proposal_single_code_hash.clone(),
        },
        &ExecuteMsg::Close { proposal_id },
        &[],
    )
    .unwrap_err()
    .downcast()
    .unwrap()
}

pub(crate) fn close_proposal(
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
            code_hash: proposal_single_code_hash.clone(),
        },
        &ExecuteMsg::Close { proposal_id },
        &[],
    )
    .unwrap();
}

pub(crate) fn mint_natives(app: &mut App, receiver: &str, amount: Vec<Coin>) {
    app.sudo(secret_multi_test::SudoMsg::Bank(BankSudo::Mint {
        to_address: receiver.to_string(),
        amount,
    }))
    .unwrap();
}

pub(crate) fn mint_snip20s(
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
            code_hash: snip20_contract_code_hash.clone(),
        },
        &snip20_base::msg::ExecuteMsg::Mint {
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

pub(crate) fn instantiate_sni20_base_default(app: &mut App) -> ContractInfo {
    let snip20_info = app.store_code(snip20_base_contract());
    let snip20_instantiate = snip20_base::msg::InstantiateMsg {
        name: "snip20 token".to_string(),
        symbol: "sniptwenty".to_string(),
        decimals: 6,
        initial_balances: Some(vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(10_000_000),
        }]),
        admin: None,
        prng_seed: to_binary(&"seed".to_string()).unwrap(),
        config: None,
        supported_denoms: None,
    };
    app.instantiate_contract(
        snip20_info,
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
    hook_code_hash: String,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash.clone(),
        },
        &ExecuteMsg::AddProposalHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash,
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
    hook_code_hash: String,
) -> ContractError {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash.clone(),
        },
        &ExecuteMsg::AddProposalHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash,
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
    hook_code_hash: String,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash.clone(),
        },
        &ExecuteMsg::RemoveProposalHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash,
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
    hook_code_hash: String,
) -> ContractError {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash.clone(),
        },
        &ExecuteMsg::RemoveProposalHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash,
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
    hook_code_hash: String,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash.clone(),
        },
        &ExecuteMsg::AddVoteHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash,
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
    hook_code_hash: String,
) -> ContractError {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash.clone(),
        },
        &ExecuteMsg::AddVoteHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash,
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
    hook_code_hash: String,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash.clone(),
        },
        &ExecuteMsg::RemoveVoteHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash,
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
    hook_code_hash: String,
) -> ContractError {
    app.execute_contract(
        Addr::unchecked(sender),
        &ContractInfo {
            address: proposal_module.clone(),
            code_hash: proposal_module_code_hash.clone(),
        },
        &ExecuteMsg::RemoveVoteHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash,
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

pub(crate) fn create_snip20_viewing_key(
    app: &mut App,
    contract_info: ContractInfo,
    info: MessageInfo,
) -> String {
    let msg = snip20_base::msg::ExecuteMsg::CreateViewingKey {
        entropy: "entropy".to_string(),
        padding: None,
    };
    let res = app
        .execute_contract(info.sender, &contract_info, &msg, &[])
        .unwrap();
    let mut viewing_key = String::new();
    let data: snip20_base::msg::ExecuteAnswer = from_binary(&res.data.unwrap()).unwrap();
    if let snip20_base::msg::ExecuteAnswer::CreateViewingKey { key } = data {
        viewing_key = key;
    };
    viewing_key
}
