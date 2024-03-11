use cosmwasm_std::{Addr, Deps, StdResult, Uint128};
use secret_toolkit::storage::Keymap;

use crate::msg::{
    AllowanceInfo, AllowanceResponse, AllowancesResponse, AllowlistResponse, DenomResponse,
    DenylistResponse, IsFrozenResponse, StatusInfo, StatusResponse,
};
use crate::state::{
    BeforeSendHookInfo, ALLOWLIST, BEFORE_SEND_HOOK_INFO, BURNER_ALLOWANCES, DENOM, DENYLIST,
    IS_FROZEN, MINTER_ALLOWANCES,
};

// Default settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

/// Returns the token denom that this contract is the admin for. Response: DenomResponse
pub fn query_denom(deps: Deps) -> StdResult<DenomResponse> {
    let denom = DENOM.load(deps.storage)?;
    Ok(DenomResponse { denom })
}

/// Returns if token transfer is disabled. Response: IsFrozenResponse
pub fn query_is_frozen(deps: Deps) -> StdResult<IsFrozenResponse> {
    let is_frozen = IS_FROZEN.load(deps.storage)?;
    Ok(IsFrozenResponse { is_frozen })
}

/// Returns the owner of the contract. Response: Ownership
pub fn query_owner(deps: Deps) -> StdResult<cw_ownable::Ownership<::cosmwasm_std::Addr>> {
    cw_ownable::get_ownership(deps.storage)
}

/// Returns the mint allowance of the specified user. Response: AllowanceResponse
pub fn query_mint_allowance(deps: Deps, address: String) -> StdResult<AllowanceResponse> {
    let allowance = MINTER_ALLOWANCES
        .get(deps.storage, &deps.api.addr_validate(&address)?)
        .unwrap_or_else(Uint128::zero);
    Ok(AllowanceResponse { allowance })
}

/// Returns the allowance of the specified address. Response: AllowanceResponse
pub fn query_burn_allowance(deps: Deps, address: String) -> StdResult<AllowanceResponse> {
    let allowance = BURNER_ALLOWANCES
        .get(deps.storage, &deps.api.addr_validate(&address)?)
        .unwrap_or_else(Uint128::zero);
    Ok(AllowanceResponse { allowance })
}

/// Helper function used in allowance list queries.
pub fn query_allowances(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
    allowances: Keymap<Addr, Uint128>,
) -> StdResult<Vec<AllowanceInfo>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let mut res: Vec<AllowanceInfo> = Vec::new();

    let mut start = start_after.clone(); // Clone start_after to mutate it if necessary

    let iter = allowances.iter(deps.storage)?;
    for item in iter {
        let (address, allowance) = item?;
        if let Some(start_after) = &start {
            if &address == start_after {
                // If we found the start point, reset it to start iterating
                start = None;
            }
        }
        if start.is_none() {
            res.push(AllowanceInfo {
                address: address.to_string(),
                allowance,
            });
            if res.len() >= limit {
                break; // Break out of loop if limit reached
            }
        }
    }

    Ok(res)
}

/// Enumerates over all allownances. Response: AllowancesResponse
pub fn query_mint_allowances(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<AllowancesResponse> {
    Ok(AllowancesResponse {
        allowances: query_allowances(deps, start_after, limit, MINTER_ALLOWANCES)?,
    })
}

/// Enumerates over all burn allownances. Response: AllowancesResponse
pub fn query_burn_allowances(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<AllowancesResponse> {
    Ok(AllowancesResponse {
        allowances: query_allowances(deps, start_after, limit, BURNER_ALLOWANCES)?,
    })
}

/// Returns wether the user is on denylist or not. Response: StatusResponse
pub fn query_is_denied(deps: Deps, address: String) -> StdResult<StatusResponse> {
    let status = DENYLIST
        .get(deps.storage, &deps.api.addr_validate(&address)?)
        .unwrap_or(false);
    Ok(StatusResponse { status })
}

/// Returns wether the user is on the allowlist or not. Response: StatusResponse
pub fn query_is_allowed(deps: Deps, address: String) -> StdResult<StatusResponse> {
    let status = ALLOWLIST
        .get(deps.storage, &deps.api.addr_validate(&address)?)
        .unwrap_or(false);
    Ok(StatusResponse { status })
}

/// Returns whether features that require MsgBeforeSendHook are enabled.
/// Most Cosmos chains do not support this feature yet.
pub fn query_before_send_hook_features(deps: Deps) -> StdResult<BeforeSendHookInfo> {
    BEFORE_SEND_HOOK_INFO.load(deps.storage)
}

/// A helper function used in list queries
pub fn query_status_map(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
    map: Keymap<Addr, bool>,
) -> StdResult<Vec<StatusInfo>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let mut res: Vec<StatusInfo> = Vec::new();

    let mut start = start_after.clone(); // Clone start_after to mutate it if necessary

    let iter = map.iter(deps.storage)?;
    for item in iter {
        let (address, status) = item?;
        if let Some(start_after) = &start {
            if &address == start_after {
                // If we found the start point, reset it to start iterating
                start = None;
            }
        }
        if start.is_none() {
            res.push(StatusInfo {
                address: address.to_string(),
                status,
            });
            if res.len() >= limit {
                break; // Break out of loop if limit reached
            }
        }
    }

    Ok(res)
}

/// Enumerates over all addresses on the allowlist. Response: AllowlistResponse
pub fn query_allowlist(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<AllowlistResponse> {
    Ok(AllowlistResponse {
        allowlist: query_status_map(deps, start_after, limit, ALLOWLIST)?,
    })
}

/// Enumerates over all addresses on the denylist. Response: DenylistResponse
pub fn query_denylist(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<DenylistResponse> {
    Ok(DenylistResponse {
        denylist: query_status_map(deps, start_after, limit, DENYLIST)?,
    })
}
