use std::cmp::min;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Addr, CosmosMsg, StdError, Uint128, WasmMsg};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InfoResponse, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::{Config, CONFIG, LAST_PAYMENT_BLOCK};
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
use secret_cw2::set_contract_version;

pub(crate) const CONTRACT_NAME: &str = "crates.io:snip20-stake-reward-distributor";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let staking_addr = deps.api.addr_validate(&msg.staking_addr)?;
    if !validate_staking(deps.as_ref(), staking_addr.clone(),msg.staking_code_hash.clone()) {
        return Err(ContractError::InvalidStakingContract {});
    }

    let reward_token = deps.api.addr_validate(&msg.reward_token)?;
    if !validate_cw20(
        deps.as_ref(),
        reward_token.clone(),
        msg.reward_token_code_hash.clone(),
    ) {
        return Err(ContractError::InvalidSnip20 {});
    }

    let config = Config {
        staking_addr: staking_addr.clone(),
        reward_token: reward_token.clone(),
        reward_rate: msg.reward_rate,
        staking_code_hash: msg.staking_code_hash.clone(),
        reward_token_code_hash: msg.reward_token_code_hash.clone(),
        reward_distributor_viewing_key: msg.reward_distributor_viewing_key.clone(),
    };
    CONFIG.save(deps.storage, &config)?;
    cw_ownable::initialize_owner(deps.storage, deps.api, Some(&msg.owner))?;

    // Initialize last payment block
    LAST_PAYMENT_BLOCK.save(deps.storage, &env.block.height)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("owner", msg.owner)
        .add_attribute("staking_addr", staking_addr.into_string())
        .add_attribute("reward_token", reward_token.into_string())
        .add_attribute("reward_rate", msg.reward_rate))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig {
            staking_addr,
            staking_code_hash,
            reward_rate,
            reward_token,
            reward_token_code_hash,
            reward_distributor_viewing_key,
        } => execute_update_config(
            deps,
            info,
            env,
            staking_addr,
            staking_code_hash,
            reward_rate,
            reward_token,
            reward_token_code_hash,
            reward_distributor_viewing_key,
        ),
        ExecuteMsg::Distribute {} => execute_distribute(deps, env),
        ExecuteMsg::Withdraw {} => execute_withdraw(deps, info, env),
        ExecuteMsg::UpdateOwnership(action) => execute_update_owner(deps, info, env, action),
    }
}
#[allow(clippy::too_many_arguments)]
pub fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    staking_addr: String,
    staking_code_hash: String,
    reward_rate: Uint128,
    reward_token: String,
    reward_token_code_hash: String,
    reward_distributor_viewing_key: String,
) -> Result<Response, ContractError> {
    cw_ownable::assert_owner(deps.storage, &info.sender)?;

    LAST_PAYMENT_BLOCK.save(deps.storage, &env.block.height)?;

    let staking_addr = deps.api.addr_validate(&staking_addr)?;
    if !validate_staking(deps.as_ref(), staking_addr.clone(),staking_code_hash.clone()) {
        return Err(ContractError::InvalidStakingContract {});
    }

    let reward_token = deps.api.addr_validate(&reward_token)?;
    if !validate_cw20(deps.as_ref(), reward_token.clone(), reward_token_code_hash.clone()) {
        return Err(ContractError::InvalidSnip20 {});
    }

    let config = Config {
        staking_addr: staking_addr.clone(),
        reward_token: reward_token.clone(),
        reward_rate,
        staking_code_hash,
        reward_token_code_hash,
        reward_distributor_viewing_key,
    };
    CONFIG.save(deps.storage, &config)?;

    let resp = match get_distribution_msg(deps.as_ref(), &env) {
        // distribution succeeded
        Ok(msg) => Response::new().add_message(msg),
        // distribution failed (either zero rewards or already distributed for block)
        _ => Response::new(),
    };

    Ok(resp
        .add_attribute("action", "update_config")
        .add_attribute("staking_addr", staking_addr.into_string())
        .add_attribute("reward_token", reward_token.into_string())
        .add_attribute("reward_rate", reward_rate))
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

pub fn validate_cw20(deps: Deps, snip20_addr: Addr, snip20_code_hash: String) -> bool {
    let response: Result<snip20_reference_impl::msg::TokenInfo, StdError> =
        deps.querier.query_wasm_smart(
            snip20_code_hash,
            snip20_addr,
            &snip20_reference_impl::msg::QueryMsg::TokenInfo {},
        );
    response.is_ok()
}

pub fn validate_staking(deps: Deps, staking_addr: Addr,staking_code_hash : String) -> bool {
    let response: Result<snip20_stake::msg::TotalValueResponse, StdError> =
        deps.querier.query_wasm_smart(
            staking_code_hash,
            staking_addr,
            &snip20_stake::msg::QueryMsg::TotalValue {  },
        );
    response.is_ok()
}

fn get_distribution_msg(deps: Deps, env: &Env) -> Result<CosmosMsg, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let last_payment_block = LAST_PAYMENT_BLOCK.load(deps.storage)?;
    if last_payment_block >= env.block.height {
        return Err(ContractError::RewardsDistributedForBlock {});
    }
    let block_diff = env.block.height - last_payment_block;

    let pending_rewards: Uint128 = config.reward_rate * Uint128::new(block_diff.into());

    let balance_info: snip20_reference_impl::msg::Balance = deps.querier.query_wasm_smart(
        config.reward_token_code_hash.clone(),
        config.reward_token.clone(),
        &snip20_reference_impl::msg::QueryMsg::Balance {
            address: env.contract.address.to_string(),
            key: config.reward_distributor_viewing_key,
        },
    )?;

    let amount = min(balance_info.amount, pending_rewards);

    if amount == Uint128::zero() {
        return Err(ContractError::ZeroRewards {});
    }

    let msg = to_binary(&snip20_reference_impl::msg::ExecuteMsg::Send {
        amount,
        msg: Some(to_binary(&snip20_stake::msg::ReceiveMsg::Fund {})?),
        recipient: config.staking_addr.clone().into_string(),
        recipient_code_hash: Some(config.staking_code_hash.clone()),
        memo: None,
        decoys: None,
        entropy: None,
        padding: None,
    })?;
    let send_msg: CosmosMsg = WasmMsg::Execute {
        contract_addr: config.reward_token.into(),
        msg,
        funds: vec![],
        code_hash: config.reward_token_code_hash.clone(),
    }
    .into();

    Ok(send_msg)
}

pub fn execute_distribute(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let msg = get_distribution_msg(deps.as_ref(), &env)?;
    LAST_PAYMENT_BLOCK.save(deps.storage, &env.block.height)?;
    Ok(Response::new()
        .add_message(msg)
        .add_attribute("action", "distribute"))
}

pub fn execute_withdraw(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
) -> Result<Response, ContractError> {
    cw_ownable::assert_owner(deps.storage, &info.sender)?;
    let config = CONFIG.load(deps.storage)?;

    let balance_info: snip20_reference_impl::msg::Balance = deps.querier.query_wasm_smart(
        config.reward_token_code_hash.clone(),
        config.reward_token.clone(),
        &snip20_reference_impl::msg::QueryMsg::Balance {
            address: env.contract.address.to_string(),
            key: config.reward_distributor_viewing_key.clone(),
        },
    )?;

    let msg = to_binary(&snip20_reference_impl::msg::ExecuteMsg::Transfer {
        // `assert_owner` call above validates that the sender is the
        // owner.
        recipient: info.sender.to_string(),
        amount: balance_info.amount,
        memo: None,
        decoys: None,
        entropy: None,
        padding: None,
    })?;
    let send_msg: CosmosMsg = WasmMsg::Execute {
        contract_addr: config.reward_token.into(),
        msg,
        funds: vec![],
        code_hash: config.reward_token_code_hash.clone(),
    }
    .into();

    Ok(Response::new()
        .add_message(send_msg)
        .add_attribute("action", "withdraw")
        .add_attribute("amount", balance_info.amount)
        .add_attribute("recipient", &info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Info {} => to_binary(&query_info(deps, env)?),
        QueryMsg::Ownership {} => to_binary(&cw_ownable::get_ownership(deps.storage)?),
    }
}

// #[cfg_attr(not(feature = "library"), entry_point)]
// pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
//     use cw20_stake_reward_distributor_v1 as v1;

//     let ContractVersion { version, .. } = get_contract_version(deps.storage)?;
//     set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

//     match msg {
//         MigrateMsg::FromV1 {} => {
//             if version == CONTRACT_VERSION {
//                 // You can not possibly be migrating from v1 to v2 and
//                 // also not changing your contract version.
//                 return Err(ContractError::AlreadyMigrated {});
//             }
//             // From v1 -> v2 we moved `owner` out of config and into
//             // the `cw_ownable` package.
//             let config = v1::state::CONFIG.load(deps.storage)?;
//             cw_ownable::initialize_owner(deps.storage, deps.api, Some(config.owner.as_str()))?;
//             let config = Config {
//                 staking_addr: config.staking_addr,
//                 reward_rate: config.reward_rate,
//                 reward_token: config.reward_token,
//             };
//             CONFIG.save(deps.storage, &config)?;

//             Ok(Response::default())
//         }
//     }
// }

fn query_info(deps: Deps, env: Env) -> StdResult<InfoResponse> {
    let config = CONFIG.load(deps.storage)?;
    let last_payment_block = LAST_PAYMENT_BLOCK.load(deps.storage)?;
    let balance_info: snip20_reference_impl::msg::Balance = deps.querier.query_wasm_smart(
        config.reward_token_code_hash.clone(),
        config.reward_token.clone(),
        &snip20_reference_impl::msg::QueryMsg::Balance {
            address: env.contract.address.to_string(),
            key: config.reward_distributor_viewing_key.clone(),
        },
    )?;

    Ok(InfoResponse {
        config,
        last_payment_block,
        balance: balance_info.amount,
    })
}
