use cosmwasm_std::{to_binary, Addr, StdResult, Storage, SubMsg, WasmMsg};
use cw_hooks::Hooks;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// An enum representing NFT staking hooks.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum NftStakeChangedHookMsg {
    Stake { addr: Addr, token_id: String },
    Unstake { addr: Addr, token_ids: Vec<String> },
}

/// Prepares NftStakeChangedHookMsg::Stake hook SubMsgs,
/// containing the address and the token_id staked.
pub fn stake_nft_hook_msgs(
    hooks: Hooks,
    storage: &dyn Storage,
    addr: Addr,
    token_id: String,
) -> StdResult<Vec<SubMsg>> {
    let msg = to_binary(&NftStakeChangedExecuteMsg::NftStakeChangeHook(
        NftStakeChangedHookMsg::Stake { addr, token_id },
    ))?;
    hooks.prepare_hooks(storage, |hook_item| {
        let execute = WasmMsg::Execute {
            contract_addr: hook_item.addr.into_string(),
            code_hash: hook_item.code_hash.clone(),
            msg: msg.clone(),
            funds: vec![],
        };
        Ok(SubMsg::new(execute))
    })
}

/// Prepares NftStakeChangedHookMsg::Unstake hook SubMsgs,
/// containing the address and the token_ids unstaked.
pub fn unstake_nft_hook_msgs(
    hooks: Hooks,
    storage: &dyn Storage,
    addr: Addr,
    token_ids: Vec<String>,
) -> StdResult<Vec<SubMsg>> {
    let msg = to_binary(&NftStakeChangedExecuteMsg::NftStakeChangeHook(
        NftStakeChangedHookMsg::Unstake { addr, token_ids },
    ))?;

    hooks.prepare_hooks(storage, |hook_item| {
        let execute = WasmMsg::Execute {
            contract_addr: hook_item.addr.into_string(),
            code_hash: hook_item.code_hash.clone(),
            msg: msg.clone(),
            funds: vec![],
        };
        Ok(SubMsg::new(execute))
    })
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum NftStakeChangedExecuteMsg {
    NftStakeChangeHook(NftStakeChangedHookMsg),
}
