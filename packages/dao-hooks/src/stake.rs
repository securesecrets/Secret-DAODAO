use cosmwasm_schema::cw_serde;
use cw_hooks::Hooks;
use cosmwasm_std::{to_binary, Addr, StdResult, Storage, SubMsg, Uint128, WasmMsg};

/// An enum representing staking hooks.
#[cw_serde]
pub enum StakeChangedHookMsg {
    Stake { addr: Addr, amount: Uint128 },
    Unstake { addr: Addr, amount: Uint128 },
}

/// Prepares StakeChangedHookMsg::Stake hook SubMsgs,
/// containing the address and the amount staked.
pub fn stake_hook_msgs(
    hooks: Hooks,
    storage: &dyn Storage,
    addr: Addr,
    amount: Uint128,
    code_hash: String,
) -> StdResult<Vec<SubMsg>> {
    let msg = to_binary(&StakeChangedExecuteMsg::StakeChangeHook(
        StakeChangedHookMsg::Stake { addr, amount },
    ))?;
    hooks.prepare_hooks(storage, |a| {
        let execute = WasmMsg::Execute {
            contract_addr: a.to_string(),
            code_hash:code_hash.clone(),
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
    code_hash: String,
) -> StdResult<Vec<SubMsg>> {
    let msg = to_binary(&StakeChangedExecuteMsg::StakeChangeHook(
        StakeChangedHookMsg::Unstake { addr, amount },
    ))?;
    hooks.prepare_hooks(storage, |a| {
        let execute = WasmMsg::Execute {
            contract_addr: a.to_string(),
            code_hash:code_hash.clone(),
            msg: msg.clone(),
            funds: vec![],
        };
        Ok(SubMsg::new(execute))
    })
}

#[cw_serde]
pub enum StakeChangedExecuteMsg {
    StakeChangeHook(StakeChangedHookMsg),
}
