#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, CosmosMsg, Deps, DepsMut, Empty, Env, MessageInfo, Response, StdResult,
    WasmMsg,
};
use secret_cw2::set_contract_version;

use dao_interface::state::{AnyContractInfo, ModuleInstantiateCallback};
use dao_pre_propose_approval_single::msg::{
    ApproverProposeMessage, ExecuteExt as ApprovalExt, ExecuteMsg as PreProposeApprovalExecuteMsg,
};
use dao_pre_propose_base::{error::PreProposeError, state::PreProposeContract};
use dao_voting::status::Status;

use crate::msg::{
    BaseInstantiateMsg, ExecuteExt, ExecuteMsg, InstantiateMsg, ProposeMessageInternal, QueryExt,
    QueryMsg,
};
use crate::state::{
    PRE_PROPOSE_APPROVAL_CONTRACT, PRE_PROPOSE_ID_TO_PROPOSAL_ID, PROPOSAL_ID_TO_PRE_PROPOSE_ID,
};

pub(crate) const CONTRACT_NAME: &str = "crates.io:dao-pre-propose-approver";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

type PrePropose = PreProposeContract<Empty, ExecuteExt, QueryExt, ApproverProposeMessage>;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, PreProposeError> {
    // This contract does not handle deposits or have open submissions
    // Here we hardcode the pre-propose-base instantiate message
    let base_instantiate_msg = BaseInstantiateMsg {
        deposit_info: None,
        open_proposal_submission: false,
        extension: Empty {},
        proposal_module_code_hash: msg.proposal_module_code_hash,
    };
    // Default pre-propose-base instantiation
    let resp = PrePropose::default().instantiate(
        deps.branch(),
        env.clone(),
        info,
        base_instantiate_msg,
    )?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Validate and save the address of the pre-propose-approval-single contract
    let addr = deps.api.addr_validate(&msg.pre_propose_approval_contract)?;
    PRE_PROPOSE_APPROVAL_CONTRACT.save(
        deps.storage,
        &AnyContractInfo {
            addr: addr.clone(),
            code_hash: msg.pre_propose_approval_contract_code_hash.clone(),
        },
    )?;

    Ok(resp.set_data(to_binary(&ModuleInstantiateCallback {
        msgs: vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: addr.clone().to_string(),
                code_hash: msg.pre_propose_approval_contract_code_hash.clone(),
                msg: to_binary(&PreProposeApprovalExecuteMsg::AddProposalSubmittedHook {
                    address: env.contract.address.to_string(),
                    code_hash: env.contract.code_hash.clone(),
                })?,
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: addr.to_string(),
                msg: to_binary(&PreProposeApprovalExecuteMsg::Extension {
                    msg: ApprovalExt::UpdateApprover {
                        address: env.contract.address.to_string(),
                    },
                })?,
                funds: vec![],
                code_hash: msg.pre_propose_approval_contract_code_hash,
            }),
        ],
    })?))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, PreProposeError> {
    match msg {
        // Override default pre-propose-base behavior
        ExecuteMsg::Propose { msg, .. } => execute_propose(deps, info, msg),
        ExecuteMsg::ProposalCompletedHook {
            proposal_id,
            new_status,
        } => execute_proposal_completed(deps, info, proposal_id, new_status),
        ExecuteMsg::Extension { msg } => match msg {
            ExecuteExt::ResetApprover {} => execute_reset_approver(deps, env, info),
        },
        _ => PrePropose::default().execute(deps, env, info, msg),
    }
}

pub fn execute_propose(
    deps: DepsMut,
    info: MessageInfo,
    msg: ApproverProposeMessage,
) -> Result<Response, PreProposeError> {
    // Check that this is coming from the expected approval contract
    let approval_contract = PRE_PROPOSE_APPROVAL_CONTRACT.load(deps.storage)?;
    if info.sender != approval_contract.addr {
        return Err(PreProposeError::Unauthorized {});
    }

    // Get pre_prospose_id, transform proposal for the approver
    // Here we make sure that there are no messages that can be executed
    let (pre_propose_id, sanitized_msg) = match msg {
        ApproverProposeMessage::Propose {
            title,
            description,
            approval_id: pre_propose_id,
        } => (
            pre_propose_id,
            ProposeMessageInternal::Propose {
                title,
                description,
                msgs: vec![],
                proposer: Some(info.sender.to_string()),
            },
        ),
    };

    let proposal_module = PrePropose::default().proposal_module.load(deps.storage)?;
    let proposal_id = deps.querier.query_wasm_smart(
        proposal_module.code_hash.clone(),
        proposal_module.addr.clone(),
        &dao_interface::proposal::Query::NextProposalId {},
    )?;
    PROPOSAL_ID_TO_PRE_PROPOSE_ID.insert(deps.storage, &proposal_id, &pre_propose_id)?;
    PRE_PROPOSE_ID_TO_PROPOSAL_ID.insert(deps.storage, &pre_propose_id, &proposal_id)?;

    let propose_messsage = WasmMsg::Execute {
        contract_addr: proposal_module.addr.into_string(),
        code_hash: proposal_module.code_hash,
        msg: to_binary(&sanitized_msg)?,
        funds: vec![],
    };
    Ok(Response::default().add_message(propose_messsage))
}

pub fn execute_proposal_completed(
    deps: DepsMut,
    info: MessageInfo,
    proposal_id: u64,
    new_status: Status,
) -> Result<Response, PreProposeError> {
    // Safety check, this message can only come from the proposal module
    let proposal_module = PrePropose::default().proposal_module.load(deps.storage)?;
    if info.sender != proposal_module.addr {
        return Err(PreProposeError::NotModule {});
    }

    // Get approval pre-propose id
    let pre_propose_id = PROPOSAL_ID_TO_PRE_PROPOSE_ID.get(deps.storage, &proposal_id);

    // Get approval contract address
    let approval_contract = PRE_PROPOSE_APPROVAL_CONTRACT.load(deps.storage)?;

    // On completion send rejection or approval message
    let msg = match new_status {
        Status::Closed => Some(WasmMsg::Execute {
            contract_addr: approval_contract.addr.clone().into_string(),
            code_hash: approval_contract.code_hash.clone(),
            msg: to_binary(&PreProposeApprovalExecuteMsg::Extension {
                msg: ApprovalExt::Reject {
                    id: pre_propose_id.unwrap(),
                },
            })?,
            funds: vec![],
        }),
        Status::Executed => Some(WasmMsg::Execute {
            contract_addr: approval_contract.addr.into_string(),
            code_hash: approval_contract.code_hash,
            msg: to_binary(&PreProposeApprovalExecuteMsg::Extension {
                msg: ApprovalExt::Approve {
                    id: pre_propose_id.unwrap(),
                },
            })?,
            funds: vec![],
        }),
        _ => None,
    };

    // If Status is not Executed or Closed, throw error
    match msg {
        Some(msg) => Ok(Response::default()
            .add_message(msg)
            .add_attribute("method", "execute_proposal_completed_hook")
            .add_attribute("proposal", proposal_id.to_string())),
        None => Err(PreProposeError::NotCompleted { status: new_status }),
    }
}

pub fn execute_reset_approver(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, PreProposeError> {
    // Check that this is coming from the DAO.
    let dao = PrePropose::default().dao.load(deps.storage)?;
    if info.sender != dao.addr {
        return Err(PreProposeError::Unauthorized {});
    }

    let pre_propose_approval_contract = PRE_PROPOSE_APPROVAL_CONTRACT.load(deps.storage)?;

    let reset_messages = vec![
        // Remove the proposal submitted hook.
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pre_propose_approval_contract.addr.clone().to_string(),
            msg: to_binary(&PreProposeApprovalExecuteMsg::RemoveProposalSubmittedHook {
                address: env.contract.address.to_string(),
                code_hash: env.contract.code_hash,
            })?,
            funds: vec![],
            code_hash: pre_propose_approval_contract.code_hash.clone(),
        }),
        // Set pre-propose-approval approver to the DAO.
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pre_propose_approval_contract.addr.to_string(),
            msg: to_binary(&PreProposeApprovalExecuteMsg::Extension {
                msg: ApprovalExt::UpdateApprover {
                    address: dao.addr.to_string(),
                },
            })?,
            funds: vec![],
            code_hash: pre_propose_approval_contract.code_hash,
        }),
    ];

    Ok(Response::default().add_messages(reset_messages))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::QueryExtension { msg } => match msg {
            QueryExt::PreProposeApprovalContract {} => {
                to_binary(&PRE_PROPOSE_APPROVAL_CONTRACT.load(deps.storage)?)
            }
            QueryExt::PreProposeApprovalIdForApproverProposalId { id } => {
                to_binary(&PROPOSAL_ID_TO_PRE_PROPOSE_ID.get(deps.storage, &id))
            }
            QueryExt::ApproverProposalIdForPreProposeApprovalId { id } => {
                to_binary(&PRE_PROPOSE_ID_TO_PROPOSAL_ID.get(deps.storage, &id))
            }
        },
        _ => PrePropose::default().query(deps, env, msg),
    }
}
