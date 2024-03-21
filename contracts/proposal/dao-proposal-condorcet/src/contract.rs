#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError, StdResult,
    SubMsgResult,
};

use dao_interface::state::AnyContractInfo;
use dao_voting::voting::{get_total_power, get_voting_power};
use secret_cw2::set_contract_version;
use secret_cw_controllers::ReplyEvent;
use shade_protocol::basic_staking::Auth;

use crate::config::UncheckedConfig;
use crate::error::ContractError;
use crate::msg::{Choice, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::proposal::{Proposal, ProposalResponse, Status};
use crate::state::{next_proposal_id, CONFIG, DAO, PROPOSAL, REPLY_IDS, TALLY, VOTE};
use crate::tally::Tally;
use crate::vote::Vote;

pub(crate) const CONTRACT_NAME: &str = "crates.io:dao-proposal-condorcet";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    DAO.save(
        deps.storage,
        &AnyContractInfo {
            addr: info.sender.clone(),
            code_hash: msg.dao_code_hash.clone(),
        },
    )?;
    CONFIG.save(deps.storage, &msg.into_checked()?)?;

    Ok(Response::default()
        .add_attribute("method", "instantiate")
        .add_attribute("creator", info.sender))
}

// the key to this contract being gas efficent [1] is that the cost of
// voting does not increase with the number of votes cast, and that
//
// ```
// gas(vote) <= gas(propose) && gas(execute) <= gas(propose)
// ```
//
// that being true, you will never be able to create a proposal that
// can not be voted on and executed inside gas limits.
//
// in terms of storage costs:
//
// propose: proposal_load + proposal_store + tally_load + tally_store + config_load
// execute: proposal_load + proposal_store + tally_load
// vote:                                     tally_load + tally_store               + vote_load + vote_store
//
// so we are good so long as:
//
// `vote_load + vote_store <= proposal_load + proposal_store + config_load`
//
// this is true so long as a vote is smaller than a proposal in
// storage which is true because proposals store `choices =
// Vec<Vec<CosmosMsg>>`, `choices.len() = vote.len()`, vote is a
// `Vec<u32>`, even an empty vec must contain it's length which is a
// usize, so `sizeof(Vec<u32>) <= sizeof(Vec<usize>) <=
// sizeof(Vec<Vec<CosmosMsg>) => sizeof(vote) <= sizeof(proposal)`.
//
// in terms of other costs:
//
// propose: query_voting_power + compute_winner [2]
// execute: query_voting_power
// vote:    query_voting_power + compute_winner
//
// so we're good there as well.
//
// [1] we need to be gas efficent in this way because the size of the
//     Tally type grows with candidates^2 and thus can be too large to
//     load from storage. we need to make sure that if this is the
//     case, the proposal fails to be created. the bad outcome we're
//     trying to avoid here is a proposal that is created but can not
//     be voted on or executed.
// [2] Tally::new computes the winner over the new matrix so that this
//     is the case.

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Propose { choices, key } => execute_propose(deps, env, info, choices, key),
        ExecuteMsg::Vote {
            proposal_id,
            vote,
            key,
        } => execute_vote(deps, env, info, proposal_id, vote, key),
        ExecuteMsg::Execute { proposal_id, key } => {
            execute_execute(deps, env, info, proposal_id, key)
        }
        ExecuteMsg::Close { proposal_id } => execute_close(deps, env, info, proposal_id),

        ExecuteMsg::SetConfig(config) => execute_set_config(deps, info, config),
    }
}

fn execute_propose(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    choices: Vec<Choice>,
    key: String,
) -> Result<Response, ContractError> {
    let dao = DAO.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;
    let auth = Auth::ViewingKey {
        key,
        address: info.sender.clone().to_string(),
    };
    let sender_voting_power = get_voting_power(
        deps.as_ref(),
        dao.code_hash.clone(),
        auth,
        &dao.addr.clone(),
        None,
    )?;
    if sender_voting_power.is_zero() {
        return Err(ContractError::ZeroVotingPower {});
    }

    let id = next_proposal_id(deps.storage)?;
    let total_power = get_total_power(deps.as_ref(), dao.code_hash.clone(), &dao.addr, None)?;

    if choices.is_empty() {
        return Err(ContractError::ZeroChoices {});
    }

    let none_of_the_above = Choice { msgs: vec![] };
    let mut choices = choices;
    choices.push(none_of_the_above);

    let tally = Tally::new(
        choices.len() as u32,
        total_power,
        env.block.height,
        config.voting_period.after(&env.block),
    );
    TALLY.insert(deps.storage, &id, &tally)?;

    let mut proposal = Proposal::new(&env.block, &config, info.sender, id, choices, total_power);
    proposal.update_status(&env.block, &tally);
    PROPOSAL.insert(deps.storage, &id, &proposal)?;

    Ok(Response::default()
        .add_attribute("method", "propose")
        .add_attribute("proposal_id", proposal.id.to_string())
        .add_attribute("proposer", proposal.proposer))
}

fn execute_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u32,
    vote: Vec<u32>,
    key: String,
) -> Result<Response, ContractError> {
    let tally = TALLY.get(deps.storage, &proposal_id);
    let auth = Auth::ViewingKey {
        key,
        address: info.sender.clone().to_string(),
    };
    let sender_power = get_voting_power(
        deps.as_ref(),
        DAO.load(deps.storage)?.code_hash.clone(),
        auth,
        &DAO.load(deps.storage)?.addr,
        Some(tally.clone().unwrap().start_height),
    )?;
    if sender_power.is_zero() {
        Err(ContractError::ZeroVotingPower {})
    } else if VOTE.contains(deps.storage, &(proposal_id, info.sender.clone())) {
        Err(ContractError::Voted {})
    } else if tally.clone().unwrap().expired(&env.block) {
        Err(ContractError::Expired {})
    } else {
        let vote = Vote::new(vote, tally.clone().unwrap().candidates())?;
        VOTE.insert(deps.storage, &(proposal_id, info.sender.clone()), &vote)?;

        tally.clone().unwrap().add_vote(vote, sender_power);
        TALLY.insert(deps.storage, &proposal_id, &tally.clone().unwrap())?;

        Ok(Response::default()
            .add_attribute("method", "vote")
            .add_attribute("proposal_id", proposal_id.to_string())
            .add_attribute("voter", info.sender)
            .add_attribute("power", sender_power))
    }
}

fn execute_execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u32,
    key: String,
) -> Result<Response, ContractError> {
    let tally = TALLY.get(deps.storage, &proposal_id);
    let dao = DAO.load(deps.storage)?;
    let auth = Auth::ViewingKey {
        key,
        address: info.sender.clone().to_string(),
    };
    let sender_power = get_voting_power(
        deps.as_ref(),
        dao.code_hash.clone(),
        auth,
        &dao.addr.clone(),
        Some(tally.clone().unwrap().start_height),
    )?;
    if sender_power.is_zero() {
        return Err(ContractError::ZeroVotingPower {});
    }

    let proposal = PROPOSAL.get(deps.storage, &proposal_id);
    if let Status::Passed { winner } = proposal
        .clone()
        .unwrap()
        .update_status(&env.block, &tally.clone().unwrap())
    {
        let msgs = proposal.clone().unwrap().set_executed(
            deps.storage,
            dao.addr,
            dao.code_hash.clone(),
            winner,
        )?;
        PROPOSAL.insert(deps.storage, &proposal_id, &proposal.clone().unwrap())?;

        Ok(Response::default()
            .add_attribute("method", "execute")
            .add_attribute("proposal_id", proposal_id.to_string())
            .add_attribute("executor", info.sender)
            .add_submessage(msgs))
    } else {
        Err(ContractError::Unexecutable {})
    }
}

fn execute_close(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u32,
) -> Result<Response, ContractError> {
    let tally = TALLY.get(deps.storage, &proposal_id);
    let proposal = PROPOSAL.get(deps.storage, &proposal_id);
    if let Status::Rejected = proposal
        .clone()
        .unwrap()
        .update_status(&env.block, &tally.unwrap())
    {
        proposal.clone().unwrap().set_closed();
        PROPOSAL.insert(deps.storage, &proposal_id, &proposal.clone().unwrap())?;

        Ok(Response::default()
            .add_attribute("method", "close")
            .add_attribute("proposal_id", proposal_id.to_string())
            .add_attribute("closer", info.sender))
    } else {
        Err(ContractError::Unclosable {})
    }
}

fn execute_set_config(
    deps: DepsMut,
    info: MessageInfo,
    config: UncheckedConfig,
) -> Result<Response, ContractError> {
    if info.sender != DAO.load(deps.storage)?.addr {
        Err(ContractError::NotDao {})
    } else {
        CONFIG.save(deps.storage, &config.into_checked()?)?;
        Ok(Response::default()
            .add_attribute("method", "update_config")
            .add_attribute("updater", info.sender))
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Proposal { id } => {
            let proposal = PROPOSAL.get(deps.storage, &id);
            let tally = TALLY.get(deps.storage, &id);
            proposal
                .clone()
                .unwrap()
                .update_status(&env.block, &tally.clone().unwrap());
            to_binary(&ProposalResponse {
                proposal: proposal.unwrap(),
                tally: tally.unwrap(),
            })
        }
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::NextProposalId {} => to_binary(&next_proposal_id(deps.storage)?),
        QueryMsg::Dao {} => to_binary(&DAO.load(deps.storage)?),
        QueryMsg::Info {} => to_binary(&dao_interface::voting::InfoResponse {
            info: secret_cw2::get_contract_version(deps.storage)?,
        }),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    let repl = REPLY_IDS.get_event(deps.storage, msg.id)?;
    match repl {
        ReplyEvent::FailedProposalExecution { proposal_id } => match msg.clone().result {
            SubMsgResult::Err(err) => Err(ContractError::Std(StdError::GenericErr { msg: err })),
            SubMsgResult::Ok(_) => {
                let proposal = PROPOSAL.get(deps.storage, &(proposal_id as u32));
                proposal.clone().unwrap().set_execution_failed();
                PROPOSAL.insert(
                    deps.storage,
                    &(proposal_id as u32),
                    &proposal.clone().unwrap(),
                )?;

                Ok(Response::default()
                    .add_attribute("proposal_execution_failed", proposal_id.to_string())
                    .add_attribute("error", msg.result.into_result().err().unwrap_or_default()))
            }
        },
        _ => unimplemented!("pre-propose and hooks not yet supported"),
    }
}
