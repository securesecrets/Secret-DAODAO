use cosmwasm_std::{to_binary, Addr, StdResult, Storage, SubMsg, Uint128, WasmMsg};
use cw_hooks::Hooks;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use shade_protocol::basic_staking::Auth;

/// An enum representing staking hooks.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StakeChangedHookMsg {
    Stake {
        addr: Addr,
        amount: Uint128,
        auth: Auth,
    },
    Unstake {
        addr: Addr,
        amount: Uint128,
        auth: Auth,
    },
}

/// Prepares StakeChangedHookMsg::Stake hook SubMsgs,
/// containing the address and the amount staked.
pub fn stake_hook_msgs(
    hooks: Hooks,
    storage: &dyn Storage,
    addr: Addr,
    amount: Uint128,
    auth: Auth,
) -> StdResult<Vec<SubMsg>> {
    let msg = to_binary(&StakeChangedExecuteMsg::StakeChangeHook(
        StakeChangedHookMsg::Stake { addr, amount, auth },
    ))?;
    hooks.prepare_hooks(storage, |hook_item| {
        let execute = WasmMsg::Execute {
            contract_addr: hook_item.addr.to_string(),
            code_hash: hook_item.code_hash.clone(),
            msg: msg.clone(),
            funds: vec![],
        };
        Ok(SubMsg::new(execute))
    })
}

/// Prepares StakeChangedHookMsg::Unstake hook SubMsgs,
/// containing the address and the amount unstaked.
pub fn unstake_hook_msgs(
    hooks: Hooks,
    storage: &dyn Storage,
    addr: Addr,
    amount: Uint128,
    auth: Auth,
) -> StdResult<Vec<SubMsg>> {
    let msg = to_binary(&StakeChangedExecuteMsg::StakeChangeHook(
        StakeChangedHookMsg::Unstake { addr, amount, auth },
    ))?;
    hooks.prepare_hooks(storage, |hook_item| {
        let execute = WasmMsg::Execute {
            contract_addr: hook_item.addr.to_string(),
            code_hash: hook_item.code_hash.clone(),
            msg: msg.clone(),
            funds: vec![],
        };
        Ok(SubMsg::new(execute))
    })
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StakeChangedExecuteMsg {
    StakeChangeHook(StakeChangedHookMsg),
}
