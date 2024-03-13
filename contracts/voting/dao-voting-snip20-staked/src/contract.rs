use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, Snip20TokenInfo, StakingInfo};
use crate::state::{
    StakingContractInfo, TokenContractInfo, ACTIVE_THRESHOLD, DAO, STAKING_CONTRACT,
    STAKING_CONTRACT_CODE_ID, STAKING_CONTRACT_UNSTAKING_DURATION, TOKEN_CONTRACT,
};
use crate::{snip20_msg, snip20_stake_msg};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdResult,
    SubMsg, SubMsgResult, Uint128, Uint256,
};
use dao_interface::state::AnyContractInfo;
use dao_interface::voting::IsActiveResponse;
use dao_voting::threshold::ActiveThreshold;
use dao_voting::threshold::ActiveThresholdResponse;
use secret_cw2::{get_contract_version, set_contract_version, ContractVersion};
use secret_toolkit::utils::InitCallback;
use secret_utils::parse_reply_event_for_contract_address;
use snip20_reference_impl::msg::QueryAnswer;
use std::convert::TryInto;

pub(crate) const CONTRACT_NAME: &str = "crates.io:dao-voting-snip20-staked";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const INSTANTIATE_TOKEN_REPLY_ID: u64 = 0;
const INSTANTIATE_STAKING_REPLY_ID: u64 = 1;

pub const PREFIX_REVOKED_PERMITS: &str = "revoked_permits";

// We multiply by this when calculating needed power for being active
// when using active threshold with percent
const PRECISION_FACTOR: u128 = 10u128.pow(9);

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    DAO.save(
        deps.storage,
        &AnyContractInfo {
            addr: info.sender.clone(),
            code_hash: msg.dao_code_hash,
        },
    )?;

    if let Some(active_threshold) = msg.active_threshold.as_ref() {
        if let ActiveThreshold::Percentage { percent } = active_threshold {
            if *percent > Decimal::percent(100) || *percent <= Decimal::percent(0) {
                return Err(ContractError::InvalidActivePercentage {});
            }
        }
        ACTIVE_THRESHOLD.save(deps.storage, active_threshold)?;
    }

    match msg.token_info {
        Snip20TokenInfo::Existing {
            address,
            code_hash,
            staking_contract,
        } => {
            let address = deps.api.addr_validate(&address)?;
            let token_contract = TokenContractInfo {
                addr: address.to_string(),
                code_hash: code_hash.clone(),
            };
            TOKEN_CONTRACT.save(deps.storage, &token_contract)?;

            if let Some(ActiveThreshold::AbsoluteCount { count }) = msg.active_threshold {
                assert_valid_absolute_count_threshold(
                    deps.as_ref(),
                    &address,
                    code_hash.clone(),
                    count,
                )?;
            }

            match staking_contract {
                StakingInfo::Existing {
                    staking_contract_address,
                    staking_contract_code_hash,
                } => {
                    let staking_contract_address =
                        deps.api.addr_validate(&staking_contract_address)?;
                    let staking_contract = StakingContractInfo {
                        addr: staking_contract_address.to_string(),
                        code_hash: staking_contract_code_hash.clone(),
                    };
                    let resp: snip20_stake::state::Config = deps.querier.query_wasm_smart(
                        staking_contract_code_hash.clone(),
                        staking_contract_address.clone(),
                        &snip20_stake::msg::QueryMsg::GetConfig {},
                    )?;

                    if address != resp.token_address {
                        return Err(ContractError::StakingContractMismatch {});
                    }

                    STAKING_CONTRACT.save(deps.storage, &staking_contract)?;
                    Ok(Response::default()
                        .add_attribute("action", "instantiate")
                        .add_attribute("token", "existing_token")
                        .add_attribute("token_address", address)
                        .add_attribute("staking_contract", staking_contract_address))
                }
                StakingInfo::New {
                    staking_code_id,
                    staking_code_hash,
                    label,
                    unstaking_duration,
                } => {
                    let init_msg = snip20_stake_msg::InstantiateMsg {
                        owner: Some(info.sender.to_string()),
                        unstaking_duration,
                        token_address: address.to_string(),
                        token_code_hash: Some(code_hash),
                    };
                    let staking_contract = StakingContractInfo {
                        addr: String::new(),
                        code_hash: staking_code_hash.clone(),
                    };

                    let msg = SubMsg::reply_always(
                        init_msg.to_cosmos_msg(
                            Some(info.sender.clone().to_string()),
                            label,
                            staking_code_id,
                            staking_code_hash,
                            None,
                        )?,
                        INSTANTIATE_STAKING_REPLY_ID,
                    );
                    STAKING_CONTRACT.save(deps.storage, &staking_contract)?;
                    Ok(Response::default()
                        .add_attribute("action", "instantiate")
                        .add_attribute("token", "existing_token")
                        .add_attribute("token_address", address)
                        .add_submessage(msg))
                }
            }
        }
        Snip20TokenInfo::New {
            code_id,
            code_hash,
            label,
            name,
            symbol,
            decimals,
            mut initial_balances,
            initial_dao_balance,
            staking_code_id,
            staking_code_hash,
            unstaking_duration,
        } => {
            let initial_supply = initial_balances
                .iter()
                .fold(Uint128::zero(), |p, n| p + n.amount);
            // Cannot instantiate with no initial token owners because
            // it would immediately lock the DAO.
            if initial_supply.is_zero() {
                return Err(ContractError::InitialBalancesError {});
            }

            // Add DAO initial balance to initial_balances vector if defined.
            if let Some(initial_dao_balance) = initial_dao_balance {
                if initial_dao_balance > Uint128::zero() {
                    let intitial_balance = snip20_msg::InitialBalance {
                        address: info.sender.to_string(),
                        amount: initial_dao_balance,
                    };
                    initial_balances.push(intitial_balance);
                }
            }
            let staking_contract = StakingContractInfo {
                addr: String::new(),
                code_hash: staking_code_hash.clone(),
            };
            let token_contract = TokenContractInfo {
                addr: String::new(),
                code_hash: code_hash.clone(),
            };

            STAKING_CONTRACT_CODE_ID.save(deps.storage, &staking_code_id)?;
            STAKING_CONTRACT_UNSTAKING_DURATION.save(deps.storage, &unstaking_duration)?;
            STAKING_CONTRACT.save(deps.storage, &staking_contract)?;
            TOKEN_CONTRACT.save(deps.storage, &token_contract)?;

            let init_msg = snip20_msg::InstantiateMsg {
                name,
                symbol,
                decimals,
                admin: Some(info.sender.clone().to_string()),
                prng_seed: to_binary(&"snip20")?,
                config: None,
                supported_denoms: None,
                initial_balances: Some(initial_balances),
            };
            let msg = SubMsg::reply_on_success(
                init_msg.to_cosmos_msg(
                    Some(info.sender.to_string()),
                    label,
                    code_id,
                    code_hash,
                    None,
                )?,
                INSTANTIATE_TOKEN_REPLY_ID,
            );

            Ok(Response::default()
                .add_attribute("action", "instantiate")
                .add_attribute("token", "new_token")
                .add_submessage(msg))
        }
    }
}

pub fn assert_valid_absolute_count_threshold(
    deps: Deps,
    token_addr: &Addr,
    token_code_hash: String,
    count: Uint128,
) -> Result<(), ContractError> {
    if count.is_zero() {
        return Err(ContractError::ZeroActiveCount {});
    }

    let token_info: QueryAnswer = deps.querier.query_wasm_smart(
        token_code_hash.clone(),
        token_addr,
        &secret_toolkit::snip20::QueryMsg::TokenInfo {},
    )?;
    let mut token_total_supply = Uint128::zero();
    match token_info {
        QueryAnswer::TokenInfo { total_supply, .. } => {
            token_total_supply = total_supply.unwrap();
        }
        _ => (),
    }

    if count > token_total_supply {
        return Err(ContractError::InvalidAbsoluteCount {});
    }
    Ok(())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateActiveThreshold { new_threshold } => {
            execute_update_active_threshold(deps, env, info, new_threshold)
        }
    }
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
                if percent > Decimal::percent(100) || percent.is_zero() {
                    return Err(ContractError::InvalidActivePercentage {});
                }
            }
            ActiveThreshold::AbsoluteCount { count } => {
                let token = TOKEN_CONTRACT.load(deps.storage)?;
                assert_valid_absolute_count_threshold(
                    deps.as_ref(),
                    &deps.api.addr_validate(&token.addr).unwrap(),
                    token.code_hash,
                    count,
                )?;
            }
        }
        ACTIVE_THRESHOLD.save(deps.storage, &active_threshold)?;
    } else {
        ACTIVE_THRESHOLD.remove(deps.storage);
    }

    Ok(Response::new().add_attribute("action", "update_active_threshold"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::TokenContract {} => query_token_contract(deps),
        QueryMsg::StakingContract {} => query_staking_contract(deps),
        QueryMsg::TotalPowerAtHeight { height } => query_total_power_at_height(deps, env, height),
        QueryMsg::Info {} => query_info(deps),
        QueryMsg::Dao {} => query_dao(deps),
        QueryMsg::IsActive {} => query_is_active(deps),
        QueryMsg::ActiveThreshold {} => query_active_threshold(deps),
        QueryMsg::VotingPowerAtHeight {
            address,
            key,
            height,
        } => query_voting_power_at_height(deps, address, key, height),
    }
}

pub fn query_token_contract(deps: Deps) -> StdResult<Binary> {
    let token = TOKEN_CONTRACT.load(deps.storage)?;
    to_binary(&token)
}

pub fn query_staking_contract(deps: Deps) -> StdResult<Binary> {
    let staking_contract = STAKING_CONTRACT.load(deps.storage)?;
    to_binary(&staking_contract)
}

pub fn query_voting_power_at_height(
    deps: Deps,
    address: String,
    key: String,
    height: Option<u64>,
) -> StdResult<Binary> {
    let staking_contract = STAKING_CONTRACT.load(deps.storage)?;
    let address = deps.api.addr_validate(&address)?;
    let res: snip20_stake::msg::StakedBalanceAtHeightResponse = deps.querier.query_wasm_smart(
        staking_contract.code_hash,
        staking_contract.addr,
        &snip20_stake::msg::QueryMsg::StakedBalanceAtHeight {
            address: address.to_string(),
            height,
            key,
        },
    )?;
    to_binary(&dao_interface::voting::VotingPowerAtHeightResponse {
        power: res.balance,
        height: res.height,
    })
}

pub fn query_total_power_at_height(
    deps: Deps,
    _env: Env,
    height: Option<u64>,
) -> StdResult<Binary> {
    let staking_contract = STAKING_CONTRACT.load(deps.storage)?;
    let res: snip20_stake::msg::TotalStakedAtHeightResponse = deps.querier.query_wasm_smart(
        staking_contract.code_hash,
        staking_contract.addr,
        &snip20_stake::msg::QueryMsg::TotalStakedAtHeight { height },
    )?;
    to_binary(&dao_interface::voting::TotalPowerAtHeightResponse {
        power: res.total,
        height: res.height,
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

pub fn query_is_active(deps: Deps) -> StdResult<Binary> {
    let threshold = ACTIVE_THRESHOLD.may_load(deps.storage)?;
    if let Some(threshold) = threshold {
        let token_contract = TOKEN_CONTRACT.load(deps.storage)?;
        let staking_contract = STAKING_CONTRACT.load(deps.storage)?;
        let actual_power: snip20_stake::msg::TotalStakedAtHeightResponse =
            deps.querier.query_wasm_smart(
                staking_contract.code_hash,
                staking_contract.addr,
                &snip20_stake::msg::QueryMsg::TotalStakedAtHeight { height: None },
            )?;
        match threshold {
            ActiveThreshold::AbsoluteCount { count } => to_binary(&IsActiveResponse {
                active: actual_power.total >= count,
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
                let token_info: QueryAnswer = deps.querier.query_wasm_smart(
                    token_contract.code_hash,
                    token_contract.addr,
                    &secret_toolkit::snip20::QueryMsg::TokenInfo {},
                )?;

                let mut total_potential_power = Uint128::zero();
                match token_info {
                    QueryAnswer::TokenInfo { total_supply, .. } => {
                        total_potential_power = total_supply.unwrap();
                    }
                    _ => (),
                }

                let total_power = total_potential_power.full_mul(PRECISION_FACTOR);
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
                    active: actual_power.total >= count,
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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_TOKEN_REPLY_ID => match msg.result {
            SubMsgResult::Ok(res) => {
                let mut token_contract = TOKEN_CONTRACT.load(deps.storage).unwrap_or_default();
                // let token_init_response: snip20_reference_impl::msg::InitResponse =
                //     from_binary(&res.data.unwrap())?;
                let token_address = parse_reply_event_for_contract_address(res.events)?;
                token_contract.addr = token_address.clone();

                let active_threshold = ACTIVE_THRESHOLD.may_load(deps.storage)?;
                if let Some(ActiveThreshold::AbsoluteCount { count }) = active_threshold {
                    assert_valid_absolute_count_threshold(
                        deps.as_ref(),
                        &deps.api.addr_validate(&token_address.clone())?,
                        token_contract.code_hash.clone(),
                        count,
                    )?;
                }

                let staking_contract_code_id = STAKING_CONTRACT_CODE_ID.load(deps.storage)?;
                let staking_contract = STAKING_CONTRACT.load(deps.storage)?;
                let unstaking_duration = STAKING_CONTRACT_UNSTAKING_DURATION.load(deps.storage)?;
                let dao_info = DAO.load(deps.storage)?;
                let init_msg = snip20_stake_msg::InstantiateMsg {
                    owner: Some(dao_info.addr.clone().to_string()),
                    unstaking_duration,
                    token_address: token_address.clone(),
                    token_code_hash: Some(token_contract.code_hash.clone()),
                };
                let msg = SubMsg::reply_on_success(
                    init_msg.to_cosmos_msg(
                        Some(dao_info.addr.clone().to_string()),
                        env.contract.address.to_string(),
                        staking_contract_code_id,
                        staking_contract.code_hash,
                        None,
                    )?,
                    INSTANTIATE_STAKING_REPLY_ID,
                );
                TOKEN_CONTRACT.save(deps.storage, &token_contract)?;
                Ok(Response::default()
                    .add_attribute("token_address", token_address)
                    .add_submessage(msg))
            }
            SubMsgResult::Err(_) => Err(ContractError::TokenInstantiateError {}),
        },
        INSTANTIATE_STAKING_REPLY_ID => match msg.result {
            SubMsgResult::Ok(resp) => {
                let mut staking_contract = STAKING_CONTRACT.load(deps.storage).unwrap_or_default();
                let staking_address = parse_reply_event_for_contract_address(resp.events)?;

                staking_contract.addr = staking_address.clone();

                STAKING_CONTRACT.save(deps.storage, &staking_contract)?;

                Ok(Response::new().add_attribute("staking contract address ", staking_address))
            }
            SubMsgResult::Err(_) => Err(ContractError::StakingInstantiateError {}),
        },

        _ => Err(ContractError::UnknownReplyId { id: msg.id }),
    }
}
