use crate::math;
use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, ReceiveMsg};
use crate::msg::{
    GetHooksResponse, ListStakersResponse, QueryMsg, Snip20ReceiveMsg,
    StakedBalanceAtHeightResponse, StakedValueResponse, StakerBalanceResponse,
    TotalStakedAtHeightResponse, TotalValueResponse,
};
use crate::state::{
    Config, StakedBalancesStore, StakedTotalStore, BALANCE, CLAIMS, CONFIG, HOOKS, MAX_CLAIMS,
    STAKED_BALANCES_PRIMARY,
};
use crate::ContractError;
use cosmwasm_std::{
    entry_point, from_binary, to_binary, Addr, Binary, Deps, DepsMut, Empty, Env, MessageInfo,
    Response, StdError, StdResult, Uint128,
};
use cw_hooks::HookItem;
use dao_hooks::stake::{stake_hook_msgs, unstake_hook_msgs};
use dao_voting::duration::validate_duration;
use secret_cw2::{get_contract_version, set_contract_version, ContractVersion};
use secret_cw_controllers::ClaimsResponse;
pub use secret_toolkit::snip20::handle::{
    burn_from_msg, burn_msg, decrease_allowance_msg, increase_allowance_msg, mint_msg,
    send_from_msg, send_msg, transfer_from_msg, transfer_msg,
};
pub use secret_toolkit::snip20::query::{allowance_query, balance_query, minters_query};
use secret_utils::Duration;
use shade_protocol::basic_staking::AuthPermit;
use shade_protocol::contract_interfaces::basic_staking::Auth;
use shade_protocol::query_auth::helpers::{
    authenticate_permit, authenticate_vk, PermitAuthentication,
};
use shade_protocol::Contract;
use snip20_reference_impl::msg::QueryAnswer;

pub(crate) const CONTRACT_NAME: &str = "crates.io:snip20-stake";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const PREFIX_REVOKED_PERMITS: &str = "revoked_permits";

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<Empty>, ContractError> {
    cw_ownable::initialize_owner(deps.storage, deps.api, msg.owner.as_deref())?;
    // Smoke test that the provided snip20 contract responds to a
    // token_info query. It is not possible to determine if the
    // contract implements the entire snip20 standard and runtime,
    // though this provides some protection against mistakes where the
    // wrong address is provided.
    let token_address = deps.api.addr_validate(&msg.token_address)?;
    let token_info: snip20_reference_impl::msg::QueryAnswer = deps.querier.query_wasm_smart(
        msg.token_code_hash.clone().unwrap(),
        &token_address,
        &secret_toolkit::snip20::QueryMsg::TokenInfo {},
    )?;
    if let QueryAnswer::TokenInfo { total_supply, .. } = token_info {
        let _supply = total_supply;
    }

    validate_duration(msg.unstaking_duration)?;

    let config = Config {
        token_address,
        token_code_hash: msg.token_code_hash.clone().unwrap(),
        unstaking_duration: msg.unstaking_duration,
        query_auth: msg.query_auth.into_valid(deps.api)?,
    };
    CONFIG.save(deps.storage, &config)?;

    // Initialize state to zero. We do this instead of using
    // `unwrap_or_default` where this is used as it protects us
    // against a scenerio where state is cleared by a bad actor and
    // `unwrap_or_default` carries on.
    BALANCE.save(deps.storage, &Uint128::zero())?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new())
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<Empty>, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => execute_receive(deps, env, info, msg),
        ExecuteMsg::Unstake { amount } => execute_unstake(deps, env, info, amount),
        ExecuteMsg::Claim {} => execute_claim(deps, env, info),
        ExecuteMsg::UpdateConfig { duration } => execute_update_config(info, deps, duration),
        ExecuteMsg::AddHook { addr, code_hash } => {
            execute_add_hook(deps, env, info, addr, code_hash)
        }
        ExecuteMsg::RemoveHook { addr, code_hash } => {
            execute_remove_hook(deps, env, info, addr, code_hash)
        }
        ExecuteMsg::UpdateOwnership(action) => execute_update_owner(deps, info, env, action),
    }
}

pub fn execute_update_config(
    info: MessageInfo,
    deps: DepsMut,
    duration: Option<Duration>,
) -> Result<Response, ContractError> {
    cw_ownable::assert_owner(deps.storage, &info.sender)?;

    validate_duration(duration)?;

    CONFIG.update(deps.storage, |mut config| -> Result<Config, StdError> {
        config.unstaking_duration = duration;
        Ok(config)
    })?;

    Ok(Response::new()
        .add_attribute("action", "update_config")
        .add_attribute(
            "unstaking_duration",
            duration
                .map(|d| format!("{d}"))
                .unwrap_or_else(|| "none".to_string()),
        ))
}

pub fn execute_receive(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    wrapper: Snip20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.token_address {
        return Err(ContractError::InvalidToken {
            received: info.sender,
            expected: config.token_address,
        });
    }
    let msg: ReceiveMsg = from_binary(&wrapper.msg.unwrap())?;
    let sender: Addr = deps.api.addr_validate(wrapper.sender.as_ref())?;
    match msg {
        ReceiveMsg::Stake {} => execute_stake(deps, env, sender, wrapper.amount),
        ReceiveMsg::Fund {} => execute_fund(deps, env, &sender, wrapper.amount),
    }
}

pub fn execute_stake(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let balance = BALANCE.load(deps.storage).unwrap_or_default();
    let staked_total = StakedTotalStore::load(deps.storage);
    let amount_to_stake = math::amount_to_stake(staked_total, balance, amount);
    let prev_balance = StakedBalancesStore::load(deps.storage, sender.clone());

    StakedBalancesStore::save(
        deps.storage,
        env.block.height,
        sender.clone(),
        prev_balance
            .checked_add(amount_to_stake)
            .map_err(StdError::overflow)?,
    )?;
    BALANCE.save(
        deps.storage,
        &balance.checked_add(amount).map_err(StdError::overflow)?,
    )?;
    StakedTotalStore::save(
        deps.storage,
        env.block.height,
        staked_total
            .checked_add(amount_to_stake)
            .map_err(StdError::overflow)?,
    )?;
    let hook_msgs = stake_hook_msgs(HOOKS, deps.storage, sender.clone(), amount_to_stake)?;
    Ok(Response::new()
        .add_submessages(hook_msgs)
        .add_attribute("action", "stake")
        .add_attribute("from", sender)
        .add_attribute("amount", amount))
}

pub fn execute_unstake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let balance = BALANCE.load(deps.storage).unwrap_or_default();
    let staked_total = StakedTotalStore::load(deps.storage);
    // invariant checks for amount_to_claim
    if staked_total.is_zero() {
        return Err(ContractError::NothingStaked {});
    }
    if amount.checked_add(balance).unwrap() == Uint128::MAX {
        return Err(ContractError::Snip20InvaraintViolation {});
    }
    if amount > staked_total {
        return Err(ContractError::ImpossibleUnstake {});
    }
    let amount_to_claim = math::amount_to_claim(staked_total, balance, amount);
    let prev_balance = StakedBalancesStore::load(deps.storage, info.sender.clone());

    StakedBalancesStore::save(
        deps.storage,
        env.block.height,
        info.sender.clone(),
        prev_balance
            .checked_sub(amount)
            .map_err(StdError::overflow)?,
    )?;
    StakedTotalStore::save(
        deps.storage,
        env.block.height,
        staked_total
            .checked_sub(amount)
            .map_err(StdError::overflow)?,
    )?;
    BALANCE.save(
        deps.storage,
        &balance
            .checked_sub(amount_to_claim)
            .map_err(StdError::overflow)?,
    )?;
    let hook_msgs = unstake_hook_msgs(HOOKS, deps.storage, info.sender.clone(), amount)?;
    match config.unstaking_duration {
        None => {
            let snip_send_msg = secret_toolkit::snip20::HandleMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: amount_to_claim,
                memo: None,
                padding: None,
            };
            let wasm_msg = cosmwasm_std::WasmMsg::Execute {
                contract_addr: config.token_address.to_string(),
                code_hash: config.token_code_hash,
                msg: to_binary(&snip_send_msg)?,
                funds: vec![],
            };
            Ok(Response::new()
                .add_message(wasm_msg)
                .add_submessages(hook_msgs)
                .add_attribute("action", "unstake")
                .add_attribute("from", info.sender)
                .add_attribute("amount", amount)
                .add_attribute("claim_duration", "None"))
        }
        Some(duration) => {
            let outstanding_claims = CLAIMS.query_claims(deps.as_ref(), &info.sender)?.claims;
            if outstanding_claims.len() + 1 > MAX_CLAIMS as usize {
                return Err(ContractError::TooManyClaims {});
            }

            CLAIMS.create_claim(
                deps.storage,
                &info.sender,
                amount_to_claim,
                duration.after(&env.block),
            )?;
            Ok(Response::new()
                .add_attribute("action", "unstake")
                .add_submessages(hook_msgs)
                .add_attribute("from", info.sender)
                .add_attribute("amount", amount)
                .add_attribute("claim_duration", format!("{duration}")))
        }
    }
}

pub fn execute_claim(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let release = CLAIMS.claim_tokens(deps.storage, &info.sender, &env.block, None)?;
    if release.is_zero() {
        return Err(ContractError::NothingToClaim {});
    }
    let config = CONFIG.load(deps.storage)?;
    let cw_send_msg = secret_toolkit::snip20::HandleMsg::Transfer {
        recipient: info.sender.to_string(),
        amount: release,
        memo: None,
        padding: None,
    };
    let wasm_msg = cosmwasm_std::WasmMsg::Execute {
        contract_addr: config.token_address.to_string(),
        code_hash: config.token_code_hash,
        msg: to_binary(&cw_send_msg)?,
        funds: vec![],
    };
    Ok(Response::new()
        .add_message(wasm_msg)
        .add_attribute("action", "claim")
        .add_attribute("from", info.sender)
        .add_attribute("amount", release))
}

pub fn execute_fund(
    deps: DepsMut,
    _env: Env,
    sender: &Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    BALANCE.update(deps.storage, |balance| -> StdResult<_> {
        balance.checked_add(amount).map_err(StdError::overflow)
    })?;
    Ok(Response::new()
        .add_attribute("action", "fund")
        .add_attribute("from", sender)
        .add_attribute("amount", amount))
}

pub fn execute_add_hook(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    addr: String,
    code_hash: String,
) -> Result<Response, ContractError> {
    cw_ownable::assert_owner(deps.storage, &info.sender)?;
    let address = deps.api.addr_validate(&addr)?;
    HOOKS.add_hook(
        deps.storage,
        HookItem {
            addr: address,
            code_hash,
        },
    )?;
    Ok(Response::new()
        .add_attribute("action", "add_hook")
        .add_attribute("hook", addr))
}

pub fn execute_remove_hook(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    addr: String,
    code_hash: String,
) -> Result<Response, ContractError> {
    cw_ownable::assert_owner(deps.storage, &info.sender)?;
    let address = deps.api.addr_validate(&addr)?;
    HOOKS.remove_hook(
        deps.storage,
        HookItem {
            addr: address,
            code_hash,
        },
    )?;
    Ok(Response::new()
        .add_attribute("action", "remove_hook")
        .add_attribute("hook", addr))
}

pub fn execute_update_owner(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    action: cw_ownable::Action,
) -> Result<Response, ContractError> {
    let ownership = cw_ownable::update_ownership(deps, &env.block, &info.sender, action)?;
    Ok(Response::default().add_attributes(ownership.into_attributes()))
}

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
        QueryMsg::TotalStakedAtHeight { height } => {
            to_binary(&query_total_staked_at_height(deps, env, height)?)
        }
        QueryMsg::TotalValue {} => to_binary(&query_total_value(deps, env)?),
        QueryMsg::GetHooks {} => to_binary(&query_hooks(deps)?),
        QueryMsg::ListStakers {} => query_list_stakers(deps),
        QueryMsg::Ownership {} => to_binary(&cw_ownable::get_ownership(deps.storage)?),
        QueryMsg::StakedBalanceAtHeight { auth, height } => {
            let query_auth = CONFIG.load(deps.storage)?.query_auth;
            let user = authenticate(deps, auth, query_auth)?;
            to_binary(&query_staked_balance_at_height(deps, env, user, height)?)
        }
        QueryMsg::StakedValue { auth } => {
            let query_auth = CONFIG.load(deps.storage)?.query_auth;
            let user = authenticate(deps, auth, query_auth)?;
            to_binary(&query_staked_value(deps, user)?)
        }
        QueryMsg::Claims { auth } => {
            let query_auth = CONFIG.load(deps.storage)?.query_auth;
            let user = authenticate(deps, auth, query_auth)?;
            to_binary(&query_claims(deps, user)?)
        }
    }
}

pub fn query_staked_balance_at_height(
    deps: Deps,
    env: Env,
    address: Addr,
    height: Option<u64>,
) -> StdResult<StakedBalanceAtHeightResponse> {
    let height = height.unwrap_or(env.block.height);
    let balance = StakedBalancesStore::may_load_at_height(deps.storage, address, height)?;
    Ok(StakedBalanceAtHeightResponse {
        balance: balance.unwrap(),
        height,
    })
}

pub fn query_total_staked_at_height(
    deps: Deps,
    _env: Env,
    height: Option<u64>,
) -> StdResult<TotalStakedAtHeightResponse> {
    let height = height.unwrap_or(_env.block.height);

    let total = StakedTotalStore::may_load_at_height(deps.storage, height)?;
    Ok(TotalStakedAtHeightResponse {
        total: total.unwrap(),
        height,
    })
}

pub fn query_staked_value(deps: Deps, address: Addr) -> StdResult<StakedValueResponse> {
    let balance = BALANCE.load(deps.storage).unwrap_or_default();
    let staked = StakedBalancesStore::load(deps.storage, address);
    let total = StakedTotalStore::load(deps.storage);
    if balance == Uint128::zero() || staked == Uint128::zero() || total == Uint128::zero() {
        Ok(StakedValueResponse {
            value: Uint128::zero(),
        })
    } else {
        let value = staked
            .checked_mul(balance)
            .map_err(StdError::overflow)?
            .checked_div(total)
            .map_err(StdError::divide_by_zero)?;
        Ok(StakedValueResponse { value })
    }
}

pub fn query_total_value(deps: Deps, _env: Env) -> StdResult<TotalValueResponse> {
    let balance = BALANCE.load(deps.storage).unwrap_or_default();
    Ok(TotalValueResponse { total: balance })
}

pub fn query_config(deps: Deps) -> StdResult<Config> {
    let config = CONFIG.load(deps.storage)?;
    Ok(config)
}

pub fn query_claims(deps: Deps, address: Addr) -> StdResult<ClaimsResponse> {
    CLAIMS.query_claims(deps, &address)
}

pub fn query_hooks(deps: Deps) -> StdResult<GetHooksResponse> {
    Ok(GetHooksResponse {
        hooks: HOOKS.query_hooks(deps)?.hooks,
    })
}

pub fn query_list_stakers(deps: Deps) -> StdResult<Binary> {
    // let start_at = start_after
    //     .map(|addr| deps.api.addr_validate(&addr))
    //     .transpose()?;
    let stakers = cw_paginate_storage::paginate_map(
        deps,
        &STAKED_BALANCES_PRIMARY,
        0,
        STAKED_BALANCES_PRIMARY
            .get_len(deps.storage)
            .unwrap_or_default(),
    )?;
    let stakers = stakers
        .into_iter()
        .map(|(address, balance)| StakerBalanceResponse {
            address: address.into_string(),
            balance,
        })
        .collect();

    to_binary(&ListStakersResponse { stakers })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let storage_version: ContractVersion = get_contract_version(deps.storage)?;

    // Only migrate if newer
    if storage_version.version.as_str() < CONTRACT_VERSION {
        // Set contract to version to latest
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    }

    Ok(Response::new().add_attribute("action", "migrate"))
}

// query auth authenticate function
pub fn authenticate(deps: Deps, auth: Auth, query_auth: Contract) -> StdResult<Addr> {
    match auth {
        Auth::ViewingKey { key, address } => {
            let address = deps.api.addr_validate(&address)?;
            if !authenticate_vk(address.clone(), key, &deps.querier, &query_auth)? {
                return Err(StdError::generic_err("Invalid Viewing Key"));
            }
            Ok(address)
        }
        Auth::Permit(permit) => {
            let res: PermitAuthentication<AuthPermit> =
                authenticate_permit(permit, &deps.querier, query_auth)?;
            if res.revoked {
                return Err(StdError::generic_err("Permit Revoked"));
            }
            Ok(res.sender)
        }
    }
}
