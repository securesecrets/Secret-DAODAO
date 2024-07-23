#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmos_sdk_proto::cosmos::bank;
use cosmwasm_std::{
    coins, to_binary, to_vec, Addr, BankMsg, Binary, ContractResult, CosmosMsg, Deps, DepsMut,
    Empty, Env, MessageInfo, QueryRequest, Response, StdError, StdResult, SystemResult, Uint128,
    Uint256,
};
use cw_hooks::HookItem;
use dao_hooks::stake::{stake_hook_msgs, unstake_hook_msgs};
use dao_interface::{
    state::AnyContractInfo,
    voting::{
        DenomResponse, IsActiveResponse, TotalPowerAtHeightResponse, VotingPowerAtHeightResponse,
    },
};
use dao_voting::{
    duration::validate_duration,
    threshold::{
        assert_valid_absolute_count_threshold, assert_valid_percentage_threshold, ActiveThreshold,
        ActiveThresholdResponse,
    },
};
use prost::Message;
use secret_cw2::{get_contract_version, set_contract_version, ContractVersion};
use secret_cw_controllers::ClaimsResponse;
use secret_utils::{must_pay, Duration};
use shade_protocol::{
    basic_staking::{Auth, AuthPermit},
    query_auth::helpers::{authenticate_permit, authenticate_vk, PermitAuthentication},
    Contract,
};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, GetHooksResponse, InstantiateMsg, MigrateMsg, QueryMsg, TokenInfo};
use crate::state::{
    Config, StakedBalancesStore, TotalStakedStore, ACTIVE_THRESHOLD, CLAIMS, CONFIG, DAO, DENOM,
    HOOKS, MAX_CLAIMS, TOKEN_ISSUER_CONTRACT,
};

pub(crate) const CONTRACT_NAME: &str = "crates.io:dao-voting-token-staked";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// We multiply by this when calculating needed power for being active
// when using active threshold with percent
const PRECISION_FACTOR: u128 = 10u128.pow(9);

pub const PREFIX_REVOKED_PERMITS: &str = "revoked_permits";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    validate_duration(msg.unstaking_duration)?;

    let config = Config {
        unstaking_duration: msg.unstaking_duration,
        query_auth: msg
            .query_auth
            .unwrap_or_default()
            .into_valid(deps.api)
            .unwrap_or_default(),
    };

    CONFIG.save(deps.storage, &config)?;
    DAO.save(
        deps.storage,
        &AnyContractInfo {
            addr: info.sender,
            code_hash: msg.dao_code_hash,
        },
    )?;

    // Validate Active Threshold
    if let Some(active_threshold) = msg.active_threshold.as_ref() {
        // Only check active threshold percentage as new tokens don't exist yet
        // We will check Absolute count (if configured) later for both existing
        // and new tokens.
        if let ActiveThreshold::Percentage { percent } = active_threshold {
            assert_valid_percentage_threshold(*percent)?;
        }
        ACTIVE_THRESHOLD.save(deps.storage, active_threshold)?;
    }

    match msg.token_info {
        TokenInfo::Existing { denom } => {
            // Validate active threshold absolute count if configured
            if let Some(ActiveThreshold::AbsoluteCount { count }) = msg.active_threshold {
                let supply = query_bank_supply_of(deps.as_ref(), denom.clone())?;
                let parsed_supply: Result<u128, _> = supply.amount.unwrap().amount.parse();

                assert_valid_absolute_count_threshold(count, parsed_supply.unwrap().into())?;
            }

            DENOM.save(deps.storage, &denom)?;

            Ok(Response::new()
                .add_attribute("action", "instantiate")
                .add_attribute("token", "existing_token")
                .add_attribute("denom", denom))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Stake { auth } => execute_stake(deps, env, info, auth),
        ExecuteMsg::Unstake { auth, amount } => execute_unstake(deps, env, info, amount, auth),
        ExecuteMsg::UpdateConfig { duration } => execute_update_config(deps, info, duration),
        ExecuteMsg::Claim {} => execute_claim(deps, env, info),
        ExecuteMsg::UpdateActiveThreshold { new_threshold } => {
            execute_update_active_threshold(deps, env, info, new_threshold)
        }
        ExecuteMsg::AddHook { addr, code_hash } => {
            execute_add_hook(deps, env, info, addr, code_hash)
        }
        ExecuteMsg::RemoveHook { addr, code_hash } => {
            execute_remove_hook(deps, env, info, addr, code_hash)
        }
    }
}

pub fn execute_stake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    auth: Auth,
) -> Result<Response, ContractError> {
    let denom = DENOM.load(deps.storage)?;
    let amount = must_pay(&info, &denom)?;

    let prev_balance = StakedBalancesStore::load(deps.storage, info.sender.clone());

    StakedBalancesStore::save(
        deps.storage,
        env.block.height,
        info.sender.clone(),
        prev_balance
            .checked_add(amount)
            .map_err(StdError::overflow)?,
    )?;

    let total_staked = TotalStakedStore::load(deps.storage);
    TotalStakedStore::save(
        deps.storage,
        env.block.height,
        total_staked
            .checked_add(amount)
            .map_err(StdError::overflow)?,
    )?;

    // Add stake hook messages
    let hook_msgs = stake_hook_msgs(HOOKS, deps.storage, info.sender.clone(), amount, auth)?;

    Ok(Response::new()
        .add_submessages(hook_msgs)
        .add_attribute("action", "stake")
        .add_attribute("amount", amount.to_string())
        .add_attribute("from", info.sender))
}

pub fn execute_unstake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    auth: Auth,
) -> Result<Response, ContractError> {
    if amount.is_zero() {
        return Err(ContractError::ZeroUnstake {});
    }

    let prev_balance = StakedBalancesStore::load(deps.storage, info.sender.clone());
    if prev_balance == Uint128::zero() || prev_balance < amount {
        return Err(ContractError::InvalidUnstakeAmount {});
    }

    StakedBalancesStore::save(
        deps.storage,
        env.block.height,
        info.sender.clone(),
        prev_balance
            .checked_sub(amount)
            .map_err(StdError::overflow)?,
    )?;

    let total_staked = TotalStakedStore::load(deps.storage);
    TotalStakedStore::save(
        deps.storage,
        env.block.height,
        total_staked
            .checked_sub(amount)
            .map_err(StdError::overflow)?,
    )?;

    // Add unstake hook messages
    let hook_msgs = unstake_hook_msgs(HOOKS, deps.storage, info.sender.clone(), amount, auth)?;

    let config = CONFIG.load(deps.storage)?;
    let denom = DENOM.load(deps.storage)?;
    match config.unstaking_duration {
        None => {
            let msg = CosmosMsg::Bank(BankMsg::Send {
                to_address: info.sender.to_string(),
                amount: coins(amount.u128(), denom),
            });
            Ok(Response::new()
                .add_message(msg)
                .add_submessages(hook_msgs)
                .add_attribute("action", "unstake")
                .add_attribute("from", info.sender)
                .add_attribute("amount", amount)
                .add_attribute("claim_duration", "None"))
        }
        Some(duration) => {
            let outstanding_claims = CLAIMS.query_claims(deps.as_ref(), &info.sender)?.claims;
            if outstanding_claims.len() >= MAX_CLAIMS as usize {
                return Err(ContractError::TooManyClaims {});
            }

            CLAIMS.create_claim(
                deps.storage,
                &info.sender,
                amount,
                duration.after(&env.block),
            )?;
            Ok(Response::new()
                .add_submessages(hook_msgs)
                .add_attribute("action", "unstake")
                .add_attribute("from", info.sender)
                .add_attribute("amount", amount)
                .add_attribute("claim_duration", format!("{duration}")))
        }
    }
}

pub fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    duration: Option<Duration>,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    // Only the DAO can update the config
    let dao = DAO.load(deps.storage)?;
    if info.sender != dao.addr {
        return Err(ContractError::Unauthorized {});
    }

    validate_duration(duration)?;

    config.unstaking_duration = duration;

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attribute("action", "update_config"))
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

    let denom = DENOM.load(deps.storage)?;
    let msg = CosmosMsg::Bank(BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: coins(release.u128(), denom),
    });

    Ok(Response::new()
        .add_message(msg)
        .add_attribute("action", "claim")
        .add_attribute("from", info.sender)
        .add_attribute("amount", release))
}

pub fn execute_update_active_threshold(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    new_active_threshold: Option<ActiveThreshold>,
) -> Result<Response, ContractError> {
    let dao = DAO.load(deps.storage)?;
    if info.sender != dao.addr {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(active_threshold) = new_active_threshold {
        match active_threshold {
            ActiveThreshold::Percentage { percent } => {
                assert_valid_percentage_threshold(percent)?;
            }
            ActiveThreshold::AbsoluteCount { count } => {
                let denom = DENOM.load(deps.storage)?;

                let supply = query_bank_supply_of(deps.as_ref(), denom.clone())?;
                let parsed_supply: Result<u128, _> = supply.amount.unwrap().amount.parse();

                assert_valid_absolute_count_threshold(count, parsed_supply.unwrap().into())?;
            }
        }
        ACTIVE_THRESHOLD.save(deps.storage, &active_threshold)?;
    } else {
        ACTIVE_THRESHOLD.remove(deps.storage);
    }

    Ok(Response::new().add_attribute("action", "update_active_threshold"))
}

pub fn execute_add_hook(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    addr: String,
    code_hash: String,
) -> Result<Response, ContractError> {
    let dao = DAO.load(deps.storage)?;
    if info.sender != dao.addr {
        return Err(ContractError::Unauthorized {});
    }

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
    let dao = DAO.load(deps.storage)?;
    if info.sender != dao.addr {
        return Err(ContractError::Unauthorized {});
    }

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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::TotalPowerAtHeight { height } => {
            to_binary(&query_total_power_at_height(deps, env, height)?)
        }
        QueryMsg::Info {} => query_info(deps),
        QueryMsg::Dao {} => query_dao(deps),
        QueryMsg::GetConfig {} => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::Denom {} => to_binary(&DenomResponse {
            denom: DENOM.load(deps.storage)?,
        }),
        QueryMsg::IsActive {} => query_is_active(deps),
        QueryMsg::ActiveThreshold {} => query_active_threshold(deps),
        QueryMsg::GetHooks {} => to_binary(&query_hooks(deps)?),
        QueryMsg::TokenContract {} => to_binary(&TOKEN_ISSUER_CONTRACT.may_load(deps.storage)?),
        QueryMsg::Claims { auth } => {
            let query_auth = CONFIG.load(deps.storage)?.query_auth;
            let user = authenticate(deps, auth, query_auth)?;
            to_binary(&query_claims(deps, user)?)
        }
        QueryMsg::VotingPowerAtHeight { auth, height } => {
            let query_auth = CONFIG.load(deps.storage)?.query_auth;
            let user = authenticate(deps, auth, query_auth)?;
            to_binary(&query_voting_power_at_height(deps, env, user, height)?)
        }
    }
}

pub fn query_voting_power_at_height(
    deps: Deps,
    env: Env,
    address: Addr,
    height: Option<u64>,
) -> StdResult<VotingPowerAtHeightResponse> {
    let height = height.unwrap_or(env.block.height);
    let power = StakedBalancesStore::may_load_at_height(deps.storage, address, height)?;
    Ok(VotingPowerAtHeightResponse {
        power: power.unwrap_or_default(),
        height,
    })
}

pub fn query_total_power_at_height(
    deps: Deps,
    env: Env,
    height: Option<u64>,
) -> StdResult<TotalPowerAtHeightResponse> {
    let height = height.unwrap_or(env.block.height);
    let power = TotalStakedStore::may_load_at_height(deps.storage, height)?;
    Ok(TotalPowerAtHeightResponse {
        power: power.unwrap(),
        height,
    })
}

pub fn query_info(deps: Deps) -> StdResult<Binary> {
    let info = secret_cw2::get_contract_version(deps.storage)?;
    to_binary(&dao_interface::voting::InfoResponse { info })
}

pub fn query_dao(deps: Deps) -> StdResult<Binary> {
    let dao = DAO.load(deps.storage)?;
    to_binary(&dao)
}

pub fn query_claims(deps: Deps, address: Addr) -> StdResult<ClaimsResponse> {
    CLAIMS.query_claims(deps, &address)
}

pub fn query_is_active(deps: Deps) -> StdResult<Binary> {
    let threshold = ACTIVE_THRESHOLD.may_load(deps.storage)?;
    if let Some(threshold) = threshold {
        let denom = DENOM.load(deps.storage)?;
        let actual_power = TotalStakedStore::load(deps.storage);
        match threshold {
            ActiveThreshold::AbsoluteCount { count } => to_binary(&IsActiveResponse {
                active: actual_power >= count,
            }),
            ActiveThreshold::Percentage { percent } => {
                // percent is bounded between [0, 100]. decimal
                // represents percents in u128 terms as p *
                // 10^15. this bounds percent between [0, 10^17].
                //
                // total_potential_power is bounded between [0, 2^128]
                // as it tracks the balances of a cw20 token which has
                // a max supply of 2^128.
                //
                // with our precision factor being 10^9:
                //
                // total_power <= 2^128 * 10^9 <= 2^256
                //
                // so we're good to put that in a u256.
                //
                // multiply_ratio promotes to a u512 under the hood,
                // so it won't overflow, multiplying by a percent less
                // than 100 is gonna make something the same size or
                // smaller, applied + 10^9 <= 2^128 * 10^9 + 10^9 <=
                // 2^256, so the top of the round won't overflow, and
                // rounding is rounding down, so the whole thing can
                // be safely unwrapped at the end of the day thank you
                // for coming to my ted talk.

                let total_potential_power = query_bank_supply_of(deps, denom)?;
                let total_potential_power_u128: Result<u128, _> =
                    total_potential_power.amount.unwrap().amount.parse();
                let total_potential_power_uint128: Uint128 =
                    total_potential_power_u128.unwrap().into();

                // let total_potential_power: cosmwasm_std::SupplyResponse =
                //     deps.querier
                //         .query(&cosmwasm_std::QueryRequest::Bank(BankQuery::Supply {
                //             denom,
                //         }))?;
                let total_power = total_potential_power_uint128.full_mul(PRECISION_FACTOR);
                // under the hood decimals are `atomics / 10^decimal_places`.
                // cosmwasm doesn't give us a Decimal * Uint256
                // implementation so we take the decimal apart and
                // multiply by the fraction.
                let applied = total_power.multiply_ratio(
                    percent.atomics(),
                    Uint256::from(10u64).pow(percent.decimal_places()),
                );
                let rounded = (applied + Uint256::from(PRECISION_FACTOR) - Uint256::from(1u128))
                    / Uint256::from(PRECISION_FACTOR);
                let count: Uint128 = rounded.try_into().unwrap();
                to_binary(&IsActiveResponse {
                    active: actual_power >= count,
                })
            }
        }
    } else {
        to_binary(&IsActiveResponse { active: true })
    }
}

pub fn query_active_threshold(deps: Deps) -> StdResult<Binary> {
    to_binary(&ActiveThresholdResponse {
        active_threshold: ACTIVE_THRESHOLD.may_load(deps.storage)?,
    })
}

pub fn query_hooks(deps: Deps) -> StdResult<GetHooksResponse> {
    Ok(GetHooksResponse {
        hooks: HOOKS.query_hooks(deps)?.hooks,
    })
}

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

pub fn make_stargate_query(
    deps: Deps,
    path: String,
    encoded_query_data: Vec<u8>,
) -> StdResult<bank::v1beta1::QuerySupplyOfResponse> {
    let raw = to_vec::<QueryRequest<Empty>>(&QueryRequest::Stargate {
        path,
        data: encoded_query_data.into(),
    })
    .map_err(|serialize_err| {
        StdError::generic_err(format!("Serializing QueryRequest: {}", serialize_err))
    })?;
    match deps.querier.raw_query(&raw) {
        SystemResult::Err(system_err) => Err(StdError::generic_err(format!(
            "Querier system error: {}",
            system_err
        ))),
        SystemResult::Ok(ContractResult::Err(contract_err)) => Err(StdError::generic_err(format!(
            "Querier contract error: {}",
            contract_err
        ))),
        // response(value) is base64 encoded bytes
        SystemResult::Ok(ContractResult::Ok(value)) => {
            let str = value.to_base64();
            deps.api
                .debug(format!("WASMDEBUG: make_stargate_query: {:?}", str).as_str());
            // from_utf8(value.as_slice())
            //     .map(|s| s.to_string())
            //     .map_err(|_e| StdError::generic_err("Unable to encode from utf8"))
            let res =
                bank::v1beta1::QuerySupplyOfResponse::decode(&value[..]).map_err(|decode_err| {
                    StdError::generic_err(format!("Decode error: {:?}", decode_err))
                })?;
            Ok(res)
        }
    }
}

fn query_bank_supply_of(
    deps: Deps,
    denom: String,
) -> StdResult<bank::v1beta1::QuerySupplyOfResponse> {
    let msg = bank::v1beta1::QuerySupplyOfRequest { denom };
    make_stargate_query(
        deps,
        "/cosmos.bank.v1beta1.Query/SupplyOf".to_string(),
        Message::encode_to_vec(&msg),
    )
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
