use cosmwasm_std::{to_binary, StdResult, Storage, SubMsg, WasmMsg};
use cw_hooks::Hooks;
use dao_voting::reply::mask_vote_hook_index;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// An enum representing vote hooks, fired when new votes are cast.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
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
) -> StdResult<Vec<SubMsg>> {
    let msg = to_binary(&VoteHookExecuteMsg::VoteHook(VoteHookMsg::NewVote {
        proposal_id,
        voter,
        vote,
    }))?;
    let mut index: u64 = 0;
    hooks.prepare_hooks(storage, |hook_item| {
        let execute = WasmMsg::Execute {
            contract_addr: hook_item.addr.to_string(),
            code_hash: hook_item.code_hash.clone(),
            msg: msg.clone(),
            funds: vec![],
        };
        let masked_index = mask_vote_hook_index(index);
        let tmp = SubMsg::reply_on_error(execute, masked_index);
        index += 1;
        Ok(tmp)
    })
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum VoteHookExecuteMsg {
    VoteHook(VoteHookMsg),
}
