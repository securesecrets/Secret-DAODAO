#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Response, StdResult, SubMsg, WasmMsg,
};
use cw_hooks::HookItem;
use dao_pre_propose_base::{
    error::PreProposeError, msg::ExecuteMsg as ExecuteBase, state::PreProposeContract,
};
use dao_voting::deposit::DepositRefundPolicy;
use dao_voting::proposal::SingleChoiceProposeMsg as ProposeMsg;
use secret_cw2::set_contract_version;

use crate::msg::{
    ApproverProposeMessage, ExecuteExt, ExecuteMsg, InstantiateExt, InstantiateMsg, ProposeMessage,
    ProposeMessageInternal, QueryExt, QueryMsg,
};
use crate::state::{
    advance_approval_id, Proposal, ProposalStatus, APPROVER, COMPLETED_PROPOSALS,
    CREATED_PROPOSAL_TO_COMPLETED_PROPOSAL, PENDING_PROPOSALS,
};

pub(crate) const CONTRACT_NAME: &str = "crates.io:dao-pre-propose-approval-single";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

type PrePropose = PreProposeContract<InstantiateExt, ExecuteExt, QueryExt, ProposeMessage>;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, PreProposeError> {
    let approver = deps.api.addr_validate(&msg.extension.approver)?;
    APPROVER.save(deps.storage, &approver)?;

    let resp = PrePropose::default().instantiate(deps.branch(), env, info, msg)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(resp.add_attribute("approver", approver.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, PreProposeError> {
    match msg {
        ExecuteMsg::Propose { msg, key } => execute_propose(deps, env, info, msg, key),

        ExecuteMsg::AddProposalSubmittedHook { address, code_hash } => {
            execute_add_approver_hook(deps, info, address, code_hash)
        }
        ExecuteMsg::RemoveProposalSubmittedHook { address, code_hash } => {
            execute_remove_approver_hook(deps, info, address, code_hash)
        }

        ExecuteMsg::Extension { msg } => match msg {
            ExecuteExt::Approve { id } => execute_approve(deps, info, id),
            ExecuteExt::Reject { id } => execute_reject(deps, info, id),
            ExecuteExt::UpdateApprover { address } => execute_update_approver(deps, info, address),
        },
        // Default pre-propose-base behavior for all other messages
        _ => PrePropose::default().execute(deps, env, info, msg),
    }
}

pub fn execute_propose(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ProposeMessage,
    key: String,
) -> Result<Response, PreProposeError> {
    let pre_propose_base = PrePropose::default();
    let config = pre_propose_base.config.load(deps.storage)?;

    pre_propose_base.check_can_submit(deps.as_ref(), info.sender.clone(), key.clone())?;

    // Take deposit, if configured.
    let deposit_messages = if let Some(ref deposit_info) = config.deposit_info {
        deposit_info.check_native_deposit_paid(&info)?;
        deposit_info.get_take_deposit_messages(&info.sender, &env.contract.address)?
    } else {
        vec![]
    };

    let approval_id = advance_approval_id(deps.storage)?;

    let propose_msg_internal = match msg {
        ProposeMessage::Propose {
            title,
            description,
            msgs,
        } => ProposeMsg {
            title,
            description,
            msgs,
            proposer: Some(info.sender.to_string()),
        },
    };

    // Prepare proposal submitted hooks msg to notify approver.  Make
    // a proposal on the approver DAO to approve this pre-proposal
    let hooks_msgs =
        pre_propose_base
            .proposal_submitted_hooks
            .prepare_hooks(deps.storage, |a| {
                let execute_msg = WasmMsg::Execute {
                    contract_addr: a.addr.into_string(),
                    code_hash: a.code_hash,
                    msg: to_binary(&ExecuteBase::<ApproverProposeMessage, Empty>::Propose {
                        msg: ApproverProposeMessage::Propose {
                            title: propose_msg_internal.title.clone(),
                            description: propose_msg_internal.description.clone(),
                            approval_id,
                        },
                        key: key.clone(),
                    })?,
                    funds: vec![],
                };
                Ok(SubMsg::new(execute_msg))
            })?;

    // Save the proposal and its information as pending.
    PENDING_PROPOSALS.insert(
        deps.storage,
        &approval_id,
        &Proposal {
            status: ProposalStatus::Pending {},
            approval_id,
            proposer: info.sender,
            msg: propose_msg_internal,
            deposit: config.deposit_info,
        },
    )?;

    Ok(Response::default()
        .add_messages(deposit_messages)
        .add_submessages(hooks_msgs)
        .add_attribute("method", "pre-propose")
        .add_attribute("id", approval_id.to_string()))
}

pub fn execute_approve(
    deps: DepsMut,
    info: MessageInfo,
    id: u64,
) -> Result<Response, PreProposeError> {
    // Check sender is the approver
    let approver = APPROVER.load(deps.storage)?;
    if approver != info.sender {
        return Err(PreProposeError::Unauthorized {});
    }

    // Load proposal and send propose message to the proposal module
    let proposal = PENDING_PROPOSALS.get(deps.storage, &id);
    match proposal {
        Some(proposal) => {
            let proposal_module = PrePropose::default().proposal_module.load(deps.storage)?;

            // Snapshot the deposit for the proposal that we're about
            // to create.
            let proposal_id = deps.querier.query_wasm_smart(
                &proposal_module.code_hash.clone(),
                proposal_module.addr.clone().to_string(),
                &dao_interface::proposal::Query::NextProposalId {},
            )?;
            PrePropose::default().deposits.insert(
                deps.storage,
                &proposal_id,
                &(proposal.deposit.clone(), proposal.proposer.clone()),
            )?;

            let propose_messsage = WasmMsg::Execute {
                contract_addr: proposal_module.addr.into_string(),
                code_hash: proposal_module.code_hash,
                msg: to_binary(&ProposeMessageInternal::Propose(proposal.msg.clone()))?,
                funds: vec![],
            };

            COMPLETED_PROPOSALS.insert(
                deps.storage,
                &id,
                &Proposal {
                    status: ProposalStatus::Approved {
                        created_proposal_id: proposal_id,
                    },
                    approval_id: proposal.approval_id,
                    proposer: proposal.proposer,
                    msg: proposal.msg,
                    deposit: proposal.deposit,
                },
            )?;
            CREATED_PROPOSAL_TO_COMPLETED_PROPOSAL.insert(deps.storage, &proposal_id, &id)?;
            PENDING_PROPOSALS.remove(deps.storage, &id)?;

            Ok(Response::default()
                .add_message(propose_messsage)
                .add_attribute("method", "proposal_approved")
                .add_attribute("approval_id", id.to_string())
                .add_attribute("proposal_id", proposal_id.to_string()))
        }
        None => Err(PreProposeError::ProposalNotFound {}),
    }
}

pub fn execute_reject(
    deps: DepsMut,
    info: MessageInfo,
    id: u64,
) -> Result<Response, PreProposeError> {
    // Check sender is the approver
    let approver = APPROVER.load(deps.storage)?;
    if approver != info.sender {
        return Err(PreProposeError::Unauthorized {});
    }

    let Proposal {
        approval_id,
        proposer,
        msg,
        deposit,
        ..
    } = PENDING_PROPOSALS
        .get(deps.storage, &id)
        .ok_or(PreProposeError::ProposalNotFound {})?;

    COMPLETED_PROPOSALS.insert(
        deps.storage,
        &id,
        &Proposal {
            status: ProposalStatus::Rejected {},
            approval_id,
            proposer: proposer.clone(),
            msg: msg.clone(),
            deposit: deposit.clone(),
        },
    )?;
    PENDING_PROPOSALS.remove(deps.storage, &id)?;

    let messages = if let Some(ref deposit_info) = deposit {
        // Refund can be issued if proposal if deposits are always
        // refunded. `OnlyPassed` and `Never` refund deposit policies
        // do not apply here.
        if deposit_info.refund_policy == DepositRefundPolicy::Always {
            deposit_info.get_return_deposit_message(&proposer)?
        } else {
            // If the proposer doesn't get the deposit, the DAO does.
            let dao = PrePropose::default().dao.load(deps.storage)?;
            deposit_info.get_return_deposit_message(&dao.addr)?
        }
    } else {
        vec![]
    };

    Ok(Response::default()
        .add_attribute("method", "proposal_rejected")
        .add_attribute("proposal", id.to_string())
        .add_attribute("deposit_info", to_binary(&deposit)?.to_string())
        .add_messages(messages))
}

pub fn execute_update_approver(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
) -> Result<Response, PreProposeError> {
    // Check sender is the approver
    let approver = APPROVER.load(deps.storage)?;
    if approver != info.sender {
        return Err(PreProposeError::Unauthorized {});
    }

    // Validate address and save new approver
    let addr = deps.api.addr_validate(&address)?;
    APPROVER.save(deps.storage, &addr)?;

    Ok(Response::default())
}

pub fn execute_add_approver_hook(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
    code_hash: String,
) -> Result<Response, PreProposeError> {
    let pre_propose_base = PrePropose::default();

    let dao = pre_propose_base.dao.load(deps.storage)?;
    let approver = APPROVER.load(deps.storage)?;

    // Check sender is the approver or the parent DAO
    if approver != info.sender && dao.addr != info.sender {
        return Err(PreProposeError::Unauthorized {});
    }

    let addr = deps.api.addr_validate(&address)?;
    pre_propose_base
        .proposal_submitted_hooks
        .add_hook(deps.storage, HookItem { addr, code_hash })?;

    Ok(Response::default())
}

pub fn execute_remove_approver_hook(
    deps: DepsMut,
    info: MessageInfo,
    address: String,
    code_hash: String,
) -> Result<Response, PreProposeError> {
    let pre_propose_base = PrePropose::default();

    let dao = pre_propose_base.dao.load(deps.storage)?;
    let approver = APPROVER.load(deps.storage)?;

    // Check sender is the approver or the parent DAO
    if approver != info.sender && dao.addr != info.sender {
        return Err(PreProposeError::Unauthorized {});
    }

    // Validate address
    let addr = deps.api.addr_validate(&address)?;

    // remove hook
    pre_propose_base
        .proposal_submitted_hooks
        .remove_hook(deps.storage, HookItem { addr, code_hash })?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::QueryExtension { msg } => match msg {
            QueryExt::Approver {} => to_binary(&APPROVER.load(deps.storage)?),
            QueryExt::IsPending { id } => {
                let pending = PENDING_PROPOSALS.get(deps.storage, &id).is_some();
                // Force load completed proposal if not pending, throwing error
                // if not found.
                if !pending {
                    COMPLETED_PROPOSALS.get(deps.storage, &id);
                }

                to_binary(&pending)
            }
            QueryExt::Proposal { id } => {
                if let Some(pending) = PENDING_PROPOSALS.get(deps.storage, &id) {
                    to_binary(&pending)
                } else {
                    // Force load completed proposal if not pending, throwing
                    // error if not found.
                    to_binary(&COMPLETED_PROPOSALS.get(deps.storage, &id))
                }
            }
            QueryExt::PendingProposal { id } => {
                to_binary(&PENDING_PROPOSALS.get(deps.storage, &id))
            }
            QueryExt::PendingProposals { start_after, limit } => {
                let mut res: Vec<Proposal> = Vec::new();
                let mut start = start_after.clone();
                let binding = &PENDING_PROPOSALS;
                let iter = binding.iter(deps.storage)?;
                for item in iter {
                    let (id, proposal) = item?;
                    if let Some(start_after) = &start {
                        if &id == start_after {
                            // If we found the start point, reset it to start iterating
                            start = None;
                        }
                    }
                    if start.is_none() {
                        res.push(proposal);
                        if res.len() >= limit.unwrap_or_default() as usize {
                            break; // Break out of loop if limit reached
                        }
                    }
                }
                to_binary(&res)
            }
            QueryExt::ReversePendingProposals {
                start_before,
                limit,
            } => {
                let mut res: Vec<Proposal> = Vec::new();
                let mut start = start_before.clone();
                let binding = &PENDING_PROPOSALS;
                let iter = binding.iter(deps.storage)?.rev(); // Iterate in reverse
                for item in iter {
                    let (id, proposal) = item?;
                    if let Some(start_before) = &start {
                        if &id == start_before {
                            // If we found the start point, reset it to start iterating
                            start = None;
                        }
                    }
                    if start.is_none() {
                        res.push(proposal);
                        if res.len() >= limit.unwrap_or_default() as usize {
                            break; // Break out of loop if limit reached
                        }
                    }
                }
                to_binary(&res)
            }

            QueryExt::CompletedProposal { id } => {
                to_binary(&COMPLETED_PROPOSALS.get(deps.storage, &id))
            }
            QueryExt::CompletedProposals { start_after, limit } => {
                let mut res: Vec<Proposal> = Vec::new();
                let mut start = start_after.clone();
                let binding = &COMPLETED_PROPOSALS;
                let iter = binding.iter(deps.storage)?;
                for item in iter {
                    let (id, proposal) = item?;
                    if let Some(start_after) = &start {
                        if &id == start_after {
                            // If we found the start point, reset it to start iterating
                            start = None;
                        }
                    }
                    if start.is_none() {
                        res.push(proposal);
                        if res.len() >= limit.unwrap_or_default() as usize {
                            break; // Break out of loop if limit reached
                        }
                    }
                }
                to_binary(&res)
            }
            QueryExt::ReverseCompletedProposals {
                start_before,
                limit,
            } => {
                let mut res: Vec<Proposal> = Vec::new();
                let mut start = start_before.clone();
                let binding = &COMPLETED_PROPOSALS;
                let iter = binding.iter(deps.storage)?.rev(); // Iterate in reverse
                for item in iter {
                    let (id, proposal) = item?;
                    if let Some(start_before) = &start {
                        if &id == start_before {
                            // If we found the start point, reset it to start iterating
                            start = None;
                        }
                    }
                    if start.is_none() {
                        res.push(proposal);
                        if res.len() >= limit.unwrap_or_default() as usize {
                            break; // Break out of loop if limit reached
                        }
                    }
                }
                to_binary(&res)
            }
            QueryExt::CompletedProposalIdForCreatedProposalId { id } => {
                to_binary(&CREATED_PROPOSAL_TO_COMPLETED_PROPOSAL.get(deps.storage, &id))
            }
        },
        _ => PrePropose::default().query(deps, env, msg),
    }
}
