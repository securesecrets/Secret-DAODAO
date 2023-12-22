use cosmwasm_schema::cw_serde;
use cw_hooks::Hooks;
use dao_voting::reply::mask_vote_hook_index;
use cosmwasm_std::{to_binary, StdResult, Storage, SubMsg, WasmMsg};

/// An enum representing vote hooks, fired when new votes are cast.
#[cw_serde]
pub enum VoteHookMsg {
    NewVote {
        proposal_id: u64,
        voter: String,
        vote: String,
    },
}

/// Prepares new vote hook messages. These messages reply on error
/// and have even reply IDs.
/// IDs are set to odd numbers to then be interleaved with the proposal hooks.
pub fn new_vote_hooks(
    hooks: Hooks,
    storage: &dyn Storage,
    proposal_id: u64,
    voter: String,
    vote: String,
    code_hash: String,
) -> StdResult<Vec<SubMsg>> {
    let msg = to_binary(&VoteHookExecuteMsg::VoteHook(VoteHookMsg::NewVote {
        proposal_id,
        voter,
        vote,
    }))?;
    let mut index: u64 = 0;
    hooks.prepare_hooks(storage, |a| {
        let execute = WasmMsg::Execute {
            contract_addr: a.to_string(),
            code_hash:code_hash.clone(),
            msg: msg.clone(),
            funds: vec![],
        };
        let masked_index = mask_vote_hook_index(index);
        let tmp = SubMsg::reply_on_error(execute, masked_index);
        index += 1;
        Ok(tmp)
    })
}

#[cw_serde]
pub enum VoteHookExecuteMsg {
    VoteHook(VoteHookMsg),
}
