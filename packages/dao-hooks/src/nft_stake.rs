use cosmwasm_schema::cw_serde;
use cw_hooks::Hooks;
use cosmwasm_std::{to_binary, Addr, StdResult, Storage, SubMsg, WasmMsg};

/// An enum representing NFT staking hooks.
#[cw_serde]
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
    code_hash: String,
) -> StdResult<Vec<SubMsg>> {
    let msg = to_binary(&NftStakeChangedExecuteMsg::NftStakeChangeHook(
        NftStakeChangedHookMsg::Stake { addr, token_id },
    ))?;
    hooks.prepare_hooks(storage, |a| {
        let execute = WasmMsg::Execute {
            contract_addr: a.into_string(),
            code_hash:code_hash.clone(),
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
    code_hash: String,
) -> StdResult<Vec<SubMsg>> {
    let msg = to_binary(&NftStakeChangedExecuteMsg::NftStakeChangeHook(
        NftStakeChangedHookMsg::Unstake { addr, token_ids },
    ))?;

    hooks.prepare_hooks(storage, |a| {
        let execute = WasmMsg::Execute {
            contract_addr: a.into_string(),
            code_hash:code_hash.clone(),
            msg: msg.clone(),
            funds: vec![],
        };
        Ok(SubMsg::new(execute))
    })
}

#[cw_serde]
pub enum NftStakeChangedExecuteMsg {
    NftStakeChangeHook(NftStakeChangedHookMsg),
}
