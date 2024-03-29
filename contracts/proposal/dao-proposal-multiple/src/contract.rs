#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Reply, Response, StdError,
    StdResult, Storage, SubMsg, SubMsgResult,
};

use cw_hooks::{HookItem, Hooks};
use dao_hooks::proposal::{
    new_proposal_hooks, proposal_completed_hooks, proposal_status_changed_hooks,
};
use dao_hooks::vote::new_vote_hooks;
use dao_interface::state::{AnyContractInfo, VotingModuleInfo};
use dao_interface::voting::IsActiveResponse;
use dao_voting::veto::{VetoConfig, VetoError};
use dao_voting::{
    multiple_choice::{
        MultipleChoiceOptions, MultipleChoiceVote, MultipleChoiceVotes, VotingStrategy,
    },
    pre_propose::{PreProposeInfo, ProposalCreationPolicy},
    proposal::{DEFAULT_LIMIT, MAX_PROPOSAL_SIZE},
    status::Status,
    voting::{get_total_power, get_voting_power, validate_voting_period},
};
use secret_cw2::set_contract_version;
use secret_cw_controllers::ReplyEvent;
use secret_toolkit::permit::{Permit, RevokedPermits};
use secret_toolkit::utils::HandleCallback;
use secret_toolkit::viewing_key::{ViewingKey, ViewingKeyStore};
use secret_utils::{parse_reply_event_for_contract_address, Duration};

use crate::msg::{CreateViewingKey, QueryWithPermit, ViewingKeyError};
use crate::state::{DAO, REPLY_IDS};
use crate::{msg::MigrateMsg, state::CREATION_POLICY};
use crate::{
    msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
    proposal::{MultipleChoiceProposal, VoteResult},
    query::{ProposalListResponse, ProposalResponse, VoteInfo, VoteListResponse, VoteResponse},
    state::{Config, BALLOTS, CONFIG, PROPOSALS, PROPOSAL_COUNT, PROPOSAL_HOOKS, VOTE_HOOKS},
    ContractError,
};

pub const CONTRACT_NAME: &str = "crates.io:dao-proposal-multiple";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const PREFIX_REVOKED_PERMITS: &str = "revoked_permits";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    msg.voting_strategy.validate()?;

    DAO.save(
        deps.storage,
        &AnyContractInfo {
            code_hash: msg.dao_code_hash,
            addr: info.sender.clone(),
        },
    )?;

    let (min_voting_period, max_voting_period) =
        validate_voting_period(msg.min_voting_period, msg.max_voting_period)?;

    let (initial_policy, pre_propose_messages) = msg
        .pre_propose_info
        .into_initial_policy_and_messages(deps.storage, info.sender.clone(), REPLY_IDS)?;

    // if veto is configured, validate its fields
    if let Some(veto_config) = &msg.veto {
        veto_config.validate(&deps.as_ref(), &max_voting_period)?;
    };

    let config = Config {
        voting_strategy: msg.voting_strategy,
        min_voting_period,
        max_voting_period,
        only_members_execute: msg.only_members_execute,
        allow_revoting: msg.allow_revoting,
        close_proposal_on_execution_failure: msg.close_proposal_on_execution_failure,
        veto: msg.veto,
    };

    // Initialize proposal count to zero so that queries return zero
    // instead of None.
    PROPOSAL_COUNT.save(deps.storage, &0)?;
    CONFIG.save(deps.storage, &config)?;
    CREATION_POLICY.save(deps.storage, &initial_policy)?;

    Ok(Response::default()
        .add_submessages(pre_propose_messages)
        .add_attribute("action", "instantiate")
        .add_attribute("dao", info.sender.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<Empty>, ContractError> {
    match msg {
        ExecuteMsg::Propose {
            title,
            description,
            choices,
            proposer,
        } => execute_propose(
            deps,
            env,
            info.sender,
            title,
            description,
            choices,
            proposer,
        ),
        ExecuteMsg::Vote {
            key,
            proposal_id,
            vote,
            rationale,
        } => execute_vote(deps, env, info, key, proposal_id, vote, rationale),
        ExecuteMsg::Execute { key, proposal_id } => {
            execute_execute(deps, env, info, key, proposal_id)
        }
        ExecuteMsg::Veto { proposal_id } => execute_veto(deps, env, info, proposal_id),
        ExecuteMsg::Close { proposal_id } => execute_close(deps, env, info, proposal_id),
        ExecuteMsg::UpdateConfig {
            voting_strategy,
            min_voting_period,
            max_voting_period,
            only_members_execute,
            allow_revoting,
            dao,
            code_hash,
            close_proposal_on_execution_failure,
            veto,
        } => execute_update_config(
            deps,
            info,
            voting_strategy,
            min_voting_period,
            max_voting_period,
            only_members_execute,
            allow_revoting,
            dao,
            code_hash,
            close_proposal_on_execution_failure,
            veto,
        ),
        ExecuteMsg::UpdatePreProposeInfo { info: new_info } => {
            execute_update_proposal_creation_policy(deps, info, new_info)
        }
        ExecuteMsg::AddProposalHook { address, code_hash } => {
            execute_add_proposal_hook(deps, env, info, address, code_hash)
        }
        ExecuteMsg::RemoveProposalHook { address, code_hash } => {
            execute_remove_proposal_hook(deps, env, info, address, code_hash)
        }
        ExecuteMsg::AddVoteHook { address, code_hash } => {
            execute_add_vote_hook(deps, env, info, address, code_hash)
        }
        ExecuteMsg::RemoveVoteHook { address, code_hash } => {
            execute_remove_vote_hook(deps, env, info, address, code_hash)
        }
        ExecuteMsg::UpdateRationale {
            proposal_id,
            rationale,
        } => execute_update_rationale(deps, info, proposal_id, rationale),
        ExecuteMsg::CreateViewingKey { entropy, .. } => try_create_key(deps, env, info, entropy),
        ExecuteMsg::SetViewingKey { key, .. } => try_set_key(deps, info, key),
        ExecuteMsg::RevokePermit { permit_name, .. } => revoke_permit(deps, info, permit_name),
    }
}

pub fn execute_propose(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    title: String,
    description: String,
    options: MultipleChoiceOptions,
    proposer: Option<String>,
) -> Result<Response<Empty>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let dao_info = DAO.load(deps.storage)?;
    let proposal_creation_policy = CREATION_POLICY.load(deps.storage)?;

    // Check that the sender is permitted to create proposals.
    if !proposal_creation_policy.is_permitted(&sender) {
        return Err(ContractError::Unauthorized {});
    }

    // Determine the appropriate proposer. If this is coming from our
    // pre-propose module, it must be specified. Otherwise, the
    // proposer should not be specified.
    let proposer = match (proposer, &proposal_creation_policy) {
        (None, ProposalCreationPolicy::Anyone {}) => sender.clone(),
        // `is_permitted` above checks that an allowed module is
        // actually sending the propose message.
        (Some(proposer), ProposalCreationPolicy::Module { .. }) => {
            deps.api.addr_validate(&proposer)?
        }
        _ => return Err(ContractError::InvalidProposer {}),
    };

    let voting_module: VotingModuleInfo = deps.querier.query_wasm_smart(
        dao_info.code_hash.clone(),
        dao_info.addr.clone(),
        &dao_interface::msg::QueryMsg::VotingModule {},
    )?;

    // Voting modules are not required to implement this
    // query. Lacking an implementation they are active by default.
    let active_resp: IsActiveResponse = deps
        .querier
        .query_wasm_smart(
            voting_module.code_hash.clone(),
            voting_module.addr.to_string().clone(),
            &dao_interface::voting::Query::IsActive {},
        )
        .unwrap_or(IsActiveResponse { active: true });

    if !active_resp.active {
        return Err(ContractError::InactiveDao {});
    }

    // Validate options.
    let checked_multiple_choice_options = options.into_checked()?.options;

    let expiration = config.max_voting_period.after(&env.block);
    let total_power = get_total_power(deps.as_ref(), dao_info.code_hash, &dao_info.addr, None)?;

    let proposal = {
        // Limit mutability to this block.
        let mut proposal = MultipleChoiceProposal {
            title,
            description,
            proposer: proposer.clone(),
            start_height: env.block.height,
            min_voting_period: config.min_voting_period.map(|min| min.after(&env.block)),
            expiration,
            voting_strategy: config.voting_strategy,
            total_power,
            status: Status::Open,
            votes: MultipleChoiceVotes::zero(checked_multiple_choice_options.len()),
            allow_revoting: config.allow_revoting,
            choices: checked_multiple_choice_options,
            veto: config.veto,
        };
        // Update the proposal's status. Addresses case where proposal
        // expires on the same block as it is created.
        proposal.update_status(&env.block)?;
        proposal
    };
    let id = advance_proposal_id(deps.storage)?;

    // Limit the size of proposals.
    //
    // The Juno mainnet has a larger limit for data that can be
    // uploaded as part of an execute message than it does for data
    // that can be queried as part of a query. This means that without
    // this check it is possible to create a proposal that can not be
    // queried.
    //
    // The size selected was determined by uploading versions of this
    // contract to the Juno mainnet until queries worked within a
    // reasonable margin of error.
    //
    // `to_vec` is the method used by cosmwasm to convert a struct
    // into it's byte representation in storage.
    let proposal_size = cosmwasm_std::to_binary(&proposal)?.len() as u64;
    if proposal_size > MAX_PROPOSAL_SIZE {
        return Err(ContractError::ProposalTooLarge {
            size: proposal_size,
            max: MAX_PROPOSAL_SIZE,
        });
    }

    PROPOSALS.insert(deps.storage, &id, &proposal)?;

    let hooks = new_proposal_hooks(PROPOSAL_HOOKS, deps.storage, id, proposer.as_str())?;

    Ok(Response::default()
        .add_submessages(hooks)
        .add_attribute("action", "propose")
        .add_attribute("sender", sender)
        .add_attribute("proposal_id", id.to_string())
        .add_attribute("status", proposal.status.to_string()))
}

pub fn execute_veto(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    let mut prop = PROPOSALS
        .get(deps.storage, &proposal_id)
        .ok_or(ContractError::NoSuchProposal { id: proposal_id })?;

    // ensure status is up to date
    prop.update_status(&env.block)?;
    let old_status = prop.status;

    let veto_config = prop
        .veto
        .as_ref()
        .ok_or(VetoError::NoVetoConfiguration {})?;

    // Check sender is vetoer
    veto_config.check_is_vetoer(&info)?;

    match prop.status {
        Status::Open => {
            // can only veto an open proposal if veto_before_passed is enabled.
            veto_config.check_veto_before_passed_enabled()?;
        }
        Status::Passed => {
            // if this proposal has veto configured but is in the passed state,
            // the timelock already expired, so provide a more specific error.
            return Err(ContractError::VetoError(VetoError::TimelockExpired {}));
        }
        Status::VetoTimelock { expiration } => {
            // vetoer can veto the proposal iff the timelock is active/not
            // expired. this should never happen since the status updates to
            // passed after the timelock expires, but let's check anyway.
            if expiration.is_expired(&env.block) {
                return Err(ContractError::VetoError(VetoError::TimelockExpired {}));
            }
        }
        // generic status error if the proposal has any other status.
        _ => {
            return Err(ContractError::VetoError(VetoError::InvalidProposalStatus {
                status: prop.status.to_string(),
            }));
        }
    }

    // Update proposal status to vetoed
    prop.status = Status::Vetoed;
    PROPOSALS.insert(deps.storage, &proposal_id, &prop)?;

    // Add proposal status change hooks
    let proposal_status_changed_hooks = proposal_status_changed_hooks(
        PROPOSAL_HOOKS,
        deps.storage,
        proposal_id,
        old_status.to_string(),
        prop.status.to_string(),
    )?;

    // Add prepropose / deposit module hook which will handle deposit refunds.
    let proposal_creation_policy = CREATION_POLICY.load(deps.storage)?;
    let proposal_completed_hooks =
        proposal_completed_hooks(proposal_creation_policy, proposal_id, prop.status)?;

    Ok(Response::new()
        .add_attribute("action", "veto")
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_submessages(proposal_status_changed_hooks)
        .add_submessages(proposal_completed_hooks))
}

pub fn execute_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    key: String,
    proposal_id: u64,
    vote: MultipleChoiceVote,
    rationale: Option<String>,
) -> Result<Response<Empty>, ContractError> {
    let dao_info = DAO.load(deps.storage)?;
    let mut prop = PROPOSALS
        .get(deps.storage, &proposal_id)
        .ok_or(ContractError::NoSuchProposal { id: proposal_id })?;

    // Check that this is a valid vote.
    if vote.option_id as usize >= prop.choices.len() {
        return Err(ContractError::InvalidVote {});
    }

    // Allow voting on proposals until they expire.
    // Voting on a non-open proposal will never change
    // their outcome as if an outcome has been determined,
    // it is because no possible sequence of votes may
    // cause a different one. This then serves to allow
    // for better tallies of opinions in the event that a
    // proposal passes or is rejected early.
    if prop.expiration.is_expired(&env.block) {
        return Err(ContractError::Expired { id: proposal_id });
    }

    let vote_power = get_voting_power(
        deps.as_ref(),
        dao_info.code_hash.clone(),
        info.sender.clone(),
        key,
        &dao_info.addr,
        Some(prop.start_height),
    )?;
    if vote_power.is_zero() {
        return Err(ContractError::NotRegistered {});
    }

    // BALLOTS.update(deps.storage, (proposal_id, &info.sender), |bal| match bal {
    //     Some(current_ballot) => {
    //         if prop.allow_revoting {
    //             if current_ballot.vote == vote {
    //                 // Don't allow casting the same vote more than
    //                 // once. This seems liable to be confusing
    //                 // behavior.
    //                 Err(ContractError::AlreadyCast {})
    //             } else {
    //                 // Remove the old vote if this is a re-vote.
    //                 prop.votes
    //                     .remove_vote(current_ballot.vote, current_ballot.power)?;
    //                 Ok(Ballot {
    //                     power: vote_power,
    //                     vote,
    //                     rationale,
    //                 })
    //             }
    //         } else {
    //             Err(ContractError::AlreadyVoted {})
    //         }
    //     }
    //     None => Ok(Ballot {
    //         vote,
    //         power: vote_power,
    //         rationale,
    //     }),
    // })?;

    let current_ballot = BALLOTS.get(deps.storage, &(proposal_id, info.sender.clone()));
    if current_ballot.clone().is_some() {
        if prop.allow_revoting {
            if current_ballot.clone().unwrap().vote == vote {
                //  Don't allow casting the same vote more than
                // once. This seems liable to be confusing
                // behavior.
                return Err(ContractError::AlreadyCast {});
            } else {
                // Remove the old vote if this is a re-vote.
                prop.votes.remove_vote(
                    current_ballot.clone().unwrap().vote,
                    current_ballot.clone().unwrap().power,
                )?;
                current_ballot.clone().unwrap().power = vote_power;
                current_ballot.clone().unwrap().vote = vote;
                current_ballot.clone().unwrap().rationale = rationale.clone();
            }
        } else {
            return Err(ContractError::AlreadyVoted {});
        }
    } else {
        current_ballot.clone().unwrap().power = vote_power;
        current_ballot.clone().unwrap().vote = vote;
        current_ballot.clone().unwrap().rationale = rationale.clone();
    }
    BALLOTS.insert(
        deps.storage,
        &(proposal_id, info.sender.clone()),
        &current_ballot.unwrap(),
    )?;

    let old_status = prop.status;

    prop.votes.add_vote(vote, vote_power)?;
    prop.update_status(&env.block)?;
    PROPOSALS.insert(deps.storage, &proposal_id, &prop)?;
    let new_status = prop.status;
    let change_hooks = proposal_status_changed_hooks(
        PROPOSAL_HOOKS,
        deps.storage,
        proposal_id,
        old_status.to_string(),
        new_status.to_string(),
    )?;
    let vote_hooks = new_vote_hooks(
        VOTE_HOOKS,
        deps.storage,
        proposal_id,
        info.sender.to_string(),
        vote.to_string(),
    )?;
    Ok(Response::default()
        .add_submessages(change_hooks)
        .add_submessages(vote_hooks)
        .add_attribute("action", "vote")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_attribute("position", vote.to_string())
        .add_attribute("status", prop.status.to_string()))
}

pub fn execute_execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    key: String,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    let mut prop = PROPOSALS
        .get(deps.storage, &proposal_id)
        .ok_or(ContractError::NoSuchProposal { id: proposal_id })?;

    let config = CONFIG.load(deps.storage)?;
    let dao_info = DAO.load(deps.storage)?;

    // determine if this sender can execute
    let mut sender_can_execute = true;
    if config.only_members_execute {
        let power = get_voting_power(
            deps.as_ref(),
            dao_info.code_hash.clone(),
            info.sender.clone(),
            key,
            &dao_info.addr.clone(),
            Some(prop.start_height),
        )?;

        sender_can_execute = !power.is_zero();
    }

    // Check here that the proposal is passed or timelocked.
    // Allow it to be executed even if it is expired so long
    // as it passed during its voting period. Allow it to be
    // executed in timelock state if early_execute is enabled
    // and the sender is the vetoer.
    prop.update_status(&env.block)?;
    let old_status = prop.status;
    match &prop.status {
        Status::Passed => {
            // if passed, verify sender can execute
            if !sender_can_execute {
                return Err(ContractError::Unauthorized {});
            }
        }
        Status::VetoTimelock { .. } => {
            let veto_config = prop
                .veto
                .as_ref()
                .ok_or(VetoError::NoVetoConfiguration {})?;

            // check that the sender is the vetoer
            if veto_config.vetoer != info.sender {
                // if the sender can normally execute, but is not the vetoer,
                // return timelocked error. otherwise return unauthorized.
                if sender_can_execute {
                    return Err(ContractError::VetoError(VetoError::Timelocked {}));
                } else {
                    return Err(ContractError::Unauthorized {});
                }
            }

            // if veto timelocked, only allow execution if early_execute enabled
            veto_config.check_early_execute_enabled()?;
        }
        _ => {
            return Err(ContractError::NotPassed {});
        }
    }

    prop.status = Status::Executed;

    PROPOSALS.insert(deps.storage, &proposal_id, &prop)?;

    let vote_result = prop.calculate_vote_result()?;
    match vote_result {
        VoteResult::Tie => Err(ContractError::Tie {}), // We don't anticipate this case as the proposal would not be in passed state, checked above.
        VoteResult::SingleWinner(winning_choice) => {
            let response = if !winning_choice.msgs.is_empty() {
                let execute_message = dao_interface::msg::ExecuteMsg::ExecuteProposalHook {
                    msgs: winning_choice.msgs,
                };
                match config.close_proposal_on_execution_failure {
                    true => {
                        let reply_id = REPLY_IDS.add_event(
                            deps.storage,
                            ReplyEvent::FailedProposalExecution { proposal_id },
                        )?;
                        Response::default().add_submessage(SubMsg::reply_on_error(
                            execute_message.to_cosmos_msg(
                                dao_info.code_hash.clone(),
                                dao_info.addr.clone().to_string(),
                                None,
                            )?,
                            reply_id,
                        ))
                    }
                    false => Response::default().add_message(execute_message.to_cosmos_msg(
                        dao_info.code_hash.clone(),
                        dao_info.addr.clone().to_string(),
                        None,
                    )?),
                }
            } else {
                Response::default()
            };

            let proposal_status_changed_hooks = proposal_status_changed_hooks(
                PROPOSAL_HOOKS,
                deps.storage,
                proposal_id,
                old_status.to_string(),
                prop.status.to_string(),
            )?;

            // Add prepropose / deposit module hook which will handle deposit refunds.
            let proposal_creation_policy = CREATION_POLICY.load(deps.storage)?;
            let proposal_completed_hooks =
                proposal_completed_hooks(proposal_creation_policy, proposal_id, prop.status)?;

            Ok(response
                .add_submessages(proposal_status_changed_hooks)
                .add_submessages(proposal_completed_hooks)
                .add_attribute("action", "execute")
                .add_attribute("sender", info.sender)
                .add_attribute("proposal_id", proposal_id.to_string())
                .add_attribute("dao", dao_info.addr.to_string()))
        }
    }
}

pub fn execute_close(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
) -> Result<Response<Empty>, ContractError> {
    let mut prop = PROPOSALS.get(deps.storage, &proposal_id).unwrap();

    prop.update_status(&env.block)?;
    if prop.status != Status::Rejected {
        return Err(ContractError::WrongCloseStatus {});
    }

    let old_status = prop.status;

    prop.status = Status::Closed;

    PROPOSALS.insert(deps.storage, &proposal_id, &prop)?;

    let proposal_status_changed_hooks = proposal_status_changed_hooks(
        PROPOSAL_HOOKS,
        deps.storage,
        proposal_id,
        old_status.to_string(),
        prop.status.to_string(),
    )?;

    // Add prepropose / deposit module hook which will handle deposit refunds.
    let proposal_creation_policy = CREATION_POLICY.load(deps.storage)?;
    let proposal_completed_hooks =
        proposal_completed_hooks(proposal_creation_policy, proposal_id, prop.status)?;

    Ok(Response::default()
        .add_submessages(proposal_status_changed_hooks)
        .add_submessages(proposal_completed_hooks)
        .add_attribute("action", "close")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string()))
}

#[allow(clippy::too_many_arguments)]
pub fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    voting_strategy: VotingStrategy,
    min_voting_period: Option<Duration>,
    max_voting_period: Duration,
    only_members_execute: bool,
    allow_revoting: bool,
    dao: String,
    code_hash: String,
    close_proposal_on_execution_failure: bool,
    veto: Option<VetoConfig>,
) -> Result<Response, ContractError> {
    let dao_info = DAO.load(deps.storage)?;

    // Only the DAO may call this method.
    if info.sender != dao_info.addr {
        return Err(ContractError::Unauthorized {});
    }

    voting_strategy.validate()?;

    let dao = deps.api.addr_validate(&dao)?;

    let (min_voting_period, max_voting_period) =
        validate_voting_period(min_voting_period, max_voting_period)?;

    // if veto is configured, validate its fields
    if let Some(veto_config) = &veto {
        veto_config.validate(&deps.as_ref(), &max_voting_period)?;
    };

    CONFIG.save(
        deps.storage,
        &Config {
            voting_strategy,
            min_voting_period,
            max_voting_period,
            only_members_execute,
            allow_revoting,
            close_proposal_on_execution_failure,
            veto,
        },
    )?;

    DAO.save(
        deps.storage,
        &AnyContractInfo {
            addr: dao,
            code_hash,
        },
    )?;

    Ok(Response::default()
        .add_attribute("action", "update_config")
        .add_attribute("sender", info.sender))
}

pub fn execute_update_proposal_creation_policy(
    deps: DepsMut,
    info: MessageInfo,
    new_info: PreProposeInfo,
) -> Result<Response, ContractError> {
    let dao_info = DAO.load(deps.storage)?;
    if dao_info.addr != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    let (initial_policy, messages) =
        new_info.into_initial_policy_and_messages(deps.storage, dao_info.addr, REPLY_IDS)?;
    CREATION_POLICY.save(deps.storage, &initial_policy)?;

    Ok(Response::default()
        .add_submessages(messages)
        .add_attribute("action", "update_proposal_creation_policy")
        .add_attribute("sender", info.sender)
        .add_attribute("new_policy", format!("{initial_policy:?}")))
}

pub fn execute_update_rationale(
    deps: DepsMut,
    info: MessageInfo,
    proposal_id: u64,
    rationale: Option<String>,
) -> Result<Response, ContractError> {
    // BALLOTS.update(
    //     deps.storage,
    //     // info.sender can't be forged so we implicitly access control
    //     // with the key.
    //     (proposal_id, &info.sender),
    //     |ballot| match ballot {
    //         Some(ballot) => Ok(Ballot {
    //             rationale: rationale.clone(),
    //             ..ballot
    //         }),
    //         None => Err(ContractError::NoSuchVote {
    //             id: proposal_id,
    //             voter: info.sender.to_string(),
    //         }),
    //     },
    // )?;

    let ballot = BALLOTS.get(deps.storage, &(proposal_id, info.sender.clone()));
    if ballot.clone().is_some() {
        ballot.clone().unwrap().rationale = rationale.clone();
    } else {
        return Err(ContractError::NoSuchVote {
            id: proposal_id,
            voter: info.sender.clone().to_string(),
        });
    }
    BALLOTS.insert(
        deps.storage,
        &(proposal_id, info.sender.clone()),
        &ballot.unwrap(),
    )?;

    Ok(Response::default()
        .add_attribute("action", "update_rationale")
        .add_attribute("sender", info.sender)
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_attribute("rationale", rationale.as_deref().unwrap_or("none")))
}

pub fn execute_add_proposal_hook(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    address: String,
    code_hash: String,
) -> Result<Response, ContractError> {
    let dao_info = DAO.load(deps.storage)?;
    if dao_info.addr != info.sender {
        // Only DAO can add hooks
        return Err(ContractError::Unauthorized {});
    }

    let validated_address = deps.api.addr_validate(&address)?;

    add_hook(PROPOSAL_HOOKS, deps.storage, validated_address, code_hash)?;

    Ok(Response::default()
        .add_attribute("action", "add_proposal_hook")
        .add_attribute("address", address))
}

pub fn execute_remove_proposal_hook(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    address: String,
    code_hash: String,
) -> Result<Response, ContractError> {
    let dao_info = DAO.load(deps.storage)?;
    if dao_info.addr != info.sender {
        // Only DAO can remove hooks
        return Err(ContractError::Unauthorized {});
    }

    let validated_address = deps.api.addr_validate(&address)?;

    remove_hook(PROPOSAL_HOOKS, deps.storage, validated_address, code_hash)?;

    Ok(Response::default()
        .add_attribute("action", "remove_proposal_hook")
        .add_attribute("address", address))
}

pub fn execute_add_vote_hook(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    address: String,
    code_hash: String,
) -> Result<Response, ContractError> {
    let dao_info = DAO.load(deps.storage)?;
    if dao_info.addr != info.sender {
        // Only DAO can add hooks
        return Err(ContractError::Unauthorized {});
    }

    let validated_address = deps.api.addr_validate(&address)?;

    add_hook(VOTE_HOOKS, deps.storage, validated_address, code_hash)?;

    Ok(Response::default()
        .add_attribute("action", "add_vote_hook")
        .add_attribute("address", address))
}

pub fn execute_remove_vote_hook(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    address: String,
    code_hash: String,
) -> Result<Response, ContractError> {
    let dao_info = DAO.load(deps.storage)?;
    if dao_info.addr != info.sender {
        // Only DAO can remove hooks
        return Err(ContractError::Unauthorized {});
    }

    let validated_address = deps.api.addr_validate(&address)?;

    remove_hook(VOTE_HOOKS, deps.storage, validated_address, code_hash)?;

    Ok(Response::default()
        .add_attribute("action", "remove_vote_hook")
        .add_attribute("address", address))
}

pub fn add_hook(
    hooks: Hooks,
    storage: &mut dyn Storage,
    validated_address: Addr,
    code_hash: String,
) -> Result<(), ContractError> {
    hooks
        .add_hook(
            storage,
            HookItem {
                addr: validated_address,
                code_hash,
            },
        )
        .map_err(ContractError::HookError)?;
    Ok(())
}

pub fn remove_hook(
    hooks: Hooks,
    storage: &mut dyn Storage,
    validated_address: Addr,
    code_hash: String,
) -> Result<(), ContractError> {
    hooks
        .remove_hook(
            storage,
            HookItem {
                addr: validated_address,
                code_hash,
            },
        )
        .map_err(ContractError::HookError)?;
    Ok(())
}

pub fn next_proposal_id(store: &dyn Storage) -> StdResult<u64> {
    Ok(PROPOSAL_COUNT.may_load(store)?.unwrap_or_default() + 1)
}

pub fn advance_proposal_id(store: &mut dyn Storage) -> StdResult<u64> {
    let id: u64 = next_proposal_id(store)?;
    PROPOSAL_COUNT.save(store, &id)?;
    Ok(id)
}

pub fn try_set_key(
    deps: DepsMut,
    info: MessageInfo,
    key: String,
) -> Result<Response, ContractError> {
    ViewingKey::set(deps.storage, info.sender.as_str(), key.as_str());
    Ok(Response::default())
}

pub fn try_create_key(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    entropy: String,
) -> Result<Response, ContractError> {
    let key = ViewingKey::create(
        deps.storage,
        &info,
        &env,
        info.sender.as_str(),
        entropy.as_ref(),
    );

    Ok(Response::new().set_data(to_binary(&CreateViewingKey { key })?))
}

fn revoke_permit(
    deps: DepsMut,
    info: MessageInfo,
    permit_name: String,
) -> Result<Response, ContractError> {
    RevokedPermits::revoke_permit(
        deps.storage,
        PREFIX_REVOKED_PERMITS,
        info.sender.as_str(),
        &permit_name,
    );

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => query_config(deps),
        QueryMsg::Proposal { proposal_id } => query_proposal(deps, env, proposal_id),
        QueryMsg::ListProposals { start_after, limit } => {
            query_list_proposals(deps, env, start_after, limit)
        }
        QueryMsg::NextProposalId {} => query_next_proposal_id(deps),
        QueryMsg::ProposalCount {} => query_proposal_count(deps),
        QueryMsg::ListVotes {
            proposal_id,
            start_after,
            limit,
        } => query_list_votes(deps, proposal_id, start_after, limit),
        QueryMsg::Info {} => query_info(deps),
        QueryMsg::ReverseProposals {
            start_before,
            limit,
        } => query_reverse_proposals(deps, env, start_before, limit),
        QueryMsg::ProposalCreationPolicy {} => query_creation_policy(deps),
        QueryMsg::ProposalHooks {} => to_binary(&PROPOSAL_HOOKS.query_hooks(deps)?),
        QueryMsg::VoteHooks {} => to_binary(&VOTE_HOOKS.query_hooks(deps)?),
        QueryMsg::Dao {} => query_dao(deps),
        QueryMsg::WithPermit { permit, query } => permit_queries(deps, env, permit, query),
        _ => viewing_keys_queries(deps, env, msg),
    }
}

fn permit_queries(
    deps: Deps,
    env: Env,
    permit: Permit,
    query: QueryWithPermit,
) -> Result<Binary, StdError> {
    // Validate permit content

    let _account = secret_toolkit::permit::validate(
        deps,
        PREFIX_REVOKED_PERMITS,
        &permit,
        env.contract.address.clone().into_string(),
        None,
    )?;

    // Permit validated! We can now execute the query.
    match query {
        QueryWithPermit::GetVote { proposal_id, voter } => {
            if !permit.check_permission(&secret_toolkit::permit::TokenPermissions::Balance) {
                return Err(StdError::generic_err(format!(
                    "No permission to query get vote, got permissions {:?}",
                    permit.params.permissions
                )));
            }

            to_binary(&query_vote(deps, proposal_id, voter)?)
        }
    }
}

pub fn viewing_keys_queries(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let (addresses, key) = msg.get_validation_params(deps.api)?;

    for address in addresses {
        let result = ViewingKey::check(deps.storage, address.as_str(), key.as_str());
        if result.is_ok() {
            return match msg {
                // Base
                QueryMsg::GetVote {
                    voter, proposal_id, ..
                } => to_binary(&query_vote(deps, proposal_id, voter)?),
                _ => panic!("This query type does not require authentication"),
            };
        }
    }

    to_binary(&ViewingKeyError {
        msg: "Wrong viewing key for this address or viewing key not set".to_string(),
    })
}

pub fn query_config(deps: Deps) -> StdResult<Binary> {
    let config = CONFIG.load(deps.storage)?;
    to_binary(&config)
}

pub fn query_dao(deps: Deps) -> StdResult<Binary> {
    let dao = DAO.load(deps.storage)?;
    to_binary(&dao)
}

pub fn query_proposal(deps: Deps, env: Env, id: u64) -> StdResult<Binary> {
    let proposal = PROPOSALS.get(deps.storage, &id);
    to_binary(&proposal.unwrap().into_response(&env.block, id)?)
}

pub fn query_creation_policy(deps: Deps) -> StdResult<Binary> {
    let policy = CREATION_POLICY.load(deps.storage)?;
    to_binary(&policy)
}

pub fn query_list_proposals(
    deps: Deps,
    _env: Env,
    start_after: Option<u64>,
    limit: Option<u64>,
) -> StdResult<Binary> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT);
    //   let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;

    let mut proposals_res: Vec<ProposalResponse> = Vec::new();

    let mut start = start_after; // Clone start_after to mutate it if necessary

    let binding = &PROPOSALS;
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
            proposals_res.push(ProposalResponse { id, proposal });
            if proposals_res.len() >= limit.try_into().unwrap() {
                break; // Break out of loop if limit reached
            }
        }
    }

    to_binary(&ProposalListResponse {
        proposals: proposals_res,
    })
}

pub fn query_reverse_proposals(
    deps: Deps,
    _env: Env,
    start_before: Option<u64>,
    limit: Option<u64>,
) -> StdResult<Binary> {
    // let limit = limit.unwrap_or(DEFAULT_LIMIT);
    // let max = start_before.map(Bound::exclusive);
    // let props: Vec<ProposalResponse> = PROPOSALS
    //     .range(deps.storage, None, max, cosmwasm_std::Order::Descending)
    //     .take(limit as usize)
    //     .collect::<Result<Vec<(u64, SingleChoiceProposal)>, _>>()?
    //     .into_iter()
    //     .map(|(id, proposal)| proposal.into_response(&env.block, id))
    //     .collect::<StdResult<Vec<ProposalResponse>>>()?;

    // to_binary(&ProposalListResponse { proposals: props })

    let limit = limit.unwrap_or(DEFAULT_LIMIT);
    //   let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;

    let mut proposals_res: Vec<ProposalResponse> = Vec::new();

    let binding = &PROPOSALS;
    let iter = binding.iter(deps.storage)?;
    for item in iter.rev() {
        let (id, proposal) = item?;
        if let Some(start_before) = start_before {
            if id < start_before {
                proposals_res.push(ProposalResponse { id, proposal });
                if proposals_res.len() >= limit as usize {
                    break; // Break out of loop if limit reached
                }
            }
        } else {
            proposals_res.push(ProposalResponse { id, proposal });
            if proposals_res.len() >= limit as usize {
                break; // Break out of loop if limit reached
            }
        }
    }

    to_binary(&ProposalListResponse {
        proposals: proposals_res,
    })
}

pub fn query_next_proposal_id(deps: Deps) -> StdResult<Binary> {
    to_binary(&next_proposal_id(deps.storage)?)
}

pub fn query_proposal_count(deps: Deps) -> StdResult<Binary> {
    let proposal_count = PROPOSAL_COUNT.load(deps.storage)?;
    to_binary(&proposal_count)
}

pub fn query_vote(deps: Deps, proposal_id: u64, voter: String) -> StdResult<Binary> {
    let voter = deps.api.addr_validate(&voter)?;
    let ballot = BALLOTS.get(deps.storage, &(proposal_id, voter.clone()));
    let vote = VoteInfo {
        voter,
        vote: ballot.clone().unwrap().vote,
        power: ballot.clone().unwrap().power,
        rationale: ballot.unwrap().rationale,
    };
    to_binary(&VoteResponse { vote: Some(vote) })
}

pub fn query_list_votes(
    deps: Deps,
    proposal_id: u64,
    start_after: Option<String>,
    limit: Option<u64>,
) -> StdResult<Binary> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT);

    let mut votes_res: Vec<VoteInfo> = Vec::new();
    let mut start = start_after.clone();

    let binding = &BALLOTS;
    let iter = binding.iter(deps.storage)?;

    for item in iter {
        let ((id, addr), ballot) = item?;
        // Check if the proposal_id matches the current proposal_id in the iteration
        if id == proposal_id {
            if let Some(start_after) = &start {
                if &addr.to_string() == start_after {
                    // If we found the start point, reset it to start iterating
                    start = None;
                }
            }
            if start.is_none() {
                votes_res.push(VoteInfo {
                    voter: addr,
                    vote: ballot.vote,
                    power: ballot.power,
                    rationale: ballot.rationale,
                });
                if votes_res.len() >= limit.try_into().unwrap() {
                    break; // Break out of loop if limit reached
                }
            }
        }
    }

    to_binary(&VoteListResponse { votes: votes_res })
}

pub fn query_info(deps: Deps) -> StdResult<Binary> {
    let info = secret_cw2::get_contract_version(deps.storage)?;
    to_binary(&dao_interface::voting::InfoResponse { info })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    let repl = REPLY_IDS.get_event(deps.storage, msg.id)?;
    match repl {
        ReplyEvent::FailedProposalExecution { proposal_id } => match msg.clone().result {
            SubMsgResult::Err(err) => Err(ContractError::Std(StdError::GenericErr { msg: err })),
            SubMsgResult::Ok(_) => {
                // PROPOSALS.update(deps.storage, proposal_id, |prop| match prop {
                //     Some(mut prop) => {
                //         prop.status = Status::ExecutionFailed;

                //         Ok(prop)
                //     }
                //     None => Err(ContractError::NoSuchProposal { id: proposal_id }),
                // })?;
                let proposals = PROPOSALS.get(deps.storage, &proposal_id);
                if proposals.clone().is_some() {
                    proposals.clone().unwrap().status = Status::ExecutionFailed;
                } else {
                    return Err(ContractError::NoSuchProposal { id: proposal_id });
                }
                PROPOSALS.insert(deps.storage, &proposal_id, &proposals.unwrap())?;

                Ok(Response::new()
                    .add_attribute("proposal_execution_failed", proposal_id.to_string())
                    .add_attribute("error", msg.result.into_result().err().unwrap_or_default()))
            }
        },
        ReplyEvent::FailedProposalHook { idx } => match msg.result {
            SubMsgResult::Err(err) => Err(ContractError::Std(StdError::GenericErr { msg: err })),
            SubMsgResult::Ok(_) => {
                let hook_item = PROPOSAL_HOOKS.remove_hook_by_index(deps.storage, idx)?;
                Ok(Response::new().add_attribute(
                    "removed_proposal_hook",
                    format!("{0}:{idx}", hook_item.addr),
                ))
            }
        },
        ReplyEvent::FailedVoteHook { idx } => match msg.result {
            SubMsgResult::Err(err) => Err(ContractError::Std(StdError::GenericErr { msg: err })),
            SubMsgResult::Ok(_) => {
                let hook_item = VOTE_HOOKS.remove_hook_by_index(deps.storage, idx)?;
                Ok(Response::new()
                    .add_attribute("removed_vote_hook", format!("{0}:{idx}", hook_item.addr)))
            }
        },
        ReplyEvent::PreProposalModuleInstantiate { code_hash } => match msg.result {
            SubMsgResult::Err(err) => Err(ContractError::Std(StdError::GenericErr { msg: err })),
            SubMsgResult::Ok(res) => {
                let contract_address = parse_reply_event_for_contract_address(res.events)?;

                let module_addr = deps.api.addr_validate(&contract_address)?;
                CREATION_POLICY.save(
                    deps.storage,
                    &ProposalCreationPolicy::Module {
                        addr: module_addr.clone(),
                        code_hash,
                    },
                )?;

                // per the cosmwasm docs, we shouldn't have to forward
                // data like this, yet here we are and it does not work if
                // we do not.
                //
                // <https://github.com/CosmWasm/cosmwasm/blob/main/SEMANTICS.md#handling-the-reply>
                match res.data {
                    Some(data) => Ok(Response::new()
                        .add_attribute("update_pre_propose_module", module_addr.clone().to_string())
                        .set_data(data)),
                    None => Ok(Response::new()
                        .add_attribute("update_pre_propose_module", module_addr.to_string())),
                }
            }
        },
        ReplyEvent::FailedPreProposeModuleHook {} => {
            let addr = match CREATION_POLICY.load(deps.storage)? {
                ProposalCreationPolicy::Anyone {} => {
                    // Something is off if we're getting this
                    // reply and we don't have a pre-propose
                    // module installed. This should be
                    // unreachable.
                    return Err(ContractError::InvalidReplyID { id: msg.id });
                }
                ProposalCreationPolicy::Module { addr, code_hash: _ } => {
                    // If we are here, our pre-propose module has
                    // errored while receiving a proposal
                    // hook. Rest in peace pre-propose module.
                    CREATION_POLICY.save(deps.storage, &ProposalCreationPolicy::Anyone {})?;
                    addr
                }
            };
            Ok(Response::new().add_attribute("failed_prepropose_hook", format!("{addr}")))
        }
        _ => Err(ContractError::UnknownReplyID {}),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}
