#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::StdError;
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Empty, Env, MessageInfo, Reply,
    Response, StdResult, SubMsg, SubMsgResult, Uint128, Uint256, WasmMsg,
};
use cw_hooks::HookItem;
use dao_hooks::nft_stake::{stake_nft_hook_msgs, unstake_nft_hook_msgs};
use dao_interface::state::AnyContractInfo;
use dao_interface::state::ModuleInstantiateCallback;
use dao_interface::{nft::NftFactoryCallback, voting::IsActiveResponse};
use dao_voting::duration::validate_duration;
use dao_voting::threshold::{
    assert_valid_absolute_count_threshold, assert_valid_percentage_threshold, ActiveThreshold,
    ActiveThresholdResponse,
};
use schemars::JsonSchema;
use secret_cw2::{get_contract_version, set_contract_version, ContractVersion};
use secret_toolkit::utils::{HandleCallback, InitCallback};
use secret_utils::parse_reply_event_for_contract_address;
use secret_utils::Duration;
use serde::{Deserialize, Serialize};
use shade_protocol::basic_staking::{Auth, AuthPermit};
use shade_protocol::query_auth::helpers::{
    authenticate_permit, authenticate_vk, PermitAuthentication,
};
use shade_protocol::Contract;

use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, NftContract, QueryMsg, Snip721ReceiveMsg,
};
use crate::snip721;
use crate::state::{
    register_staked_nft, register_unstaked_nfts, Config, NftBalancesStore, StakedNftsTotalStore,
    ACTIVE_THRESHOLD, CONFIG, DAO, HOOKS, INITIAL_NFTS, MAX_CLAIMS, NFT_CLAIMS,
};

pub(crate) const CONTRACT_NAME: &str = "crates.io:dao-voting-snip721-staked";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const INSTANTIATE_NFT_CONTRACT_REPLY_ID: u64 = 0;
const VALIDATE_SUPPLY_REPLY_ID: u64 = 1;
const FACTORY_EXECUTE_REPLY_ID: u64 = 2;

// We multiply by this when calculating needed power for being active
// when using active threshold with percent
const PRECISION_FACTOR: u128 = 10u128.pow(9);

pub const PREFIX_REVOKED_PERMITS: &str = "revoked_permits";

#[derive(Serialize, Deserialize, JsonSchema)]
// Supported NFT instantiation messages
pub enum NftInstantiateMsg {
    Snip721(snip721::Snip721InstantiateMsg),
}

impl InitCallback for NftInstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}

impl NftInstantiateMsg {
    fn modify_instantiate_msg(&mut self, minter: &str) {
        match self {
            // Update minter for cw721 NFTs
            NftInstantiateMsg::Snip721(msg) => msg.admin = Some(minter.to_string()),
        }
    }

    // fn to_binary(&self) -> Result<Binary, StdError> {
    //     match self {
    //         NftInstantiateMsg::Snip721(msg) => to_binary(&msg),
    //     }
    // }
}

pub fn try_deserialize_nft_instantiate_msg(
    instantiate_msg: Binary,
) -> Result<NftInstantiateMsg, ContractError> {
    if let Ok(snip721_msg) = from_binary::<snip721::Snip721InstantiateMsg>(&instantiate_msg) {
        return Ok(NftInstantiateMsg::Snip721(snip721_msg));
    }

    Err(ContractError::NftInstantiateError {})
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<Empty>, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    DAO.save(
        deps.storage,
        &AnyContractInfo {
            addr: info.sender.clone(),
            code_hash: msg.dao_code_hash,
        },
    )?;

    // Validate unstaking duration
    validate_duration(msg.unstaking_duration)?;

    // Validate active threshold if configured
    if let Some(active_threshold) = msg.active_threshold.as_ref() {
        match active_threshold {
            ActiveThreshold::Percentage { percent } => {
                assert_valid_percentage_threshold(*percent)?;
            }
            ActiveThreshold::AbsoluteCount { count } => {
                // Check Absolute count is less than the supply of NFTs for existing
                // NFT contracts. For new NFT contracts, we will check this in the reply.
                if let NftContract::Existing {
                    ref address,
                    ref code_hash,
                } = msg.nft_contract
                {
                    let nft_supply: snip721::NumTokens = deps.querier.query_wasm_smart(
                        code_hash,
                        address,
                        &snip721::Snip721QueryMsg::NumTokens { viewer: None },
                    )?;
                    // Check the absolute count is less than the supply of NFTs and
                    // greater than zero.
                    assert_valid_absolute_count_threshold(
                        *count,
                        Uint128::new(nft_supply.count.into()),
                    )?;
                }
            }
        }
        ACTIVE_THRESHOLD.save(deps.storage, active_threshold)?;
    }

    StakedNftsTotalStore::save(deps.storage, env.block.height, Uint128::zero())?;

    match msg.nft_contract {
        NftContract::Existing { address, code_hash } => {
            let config = Config {
                nft_address: deps.api.addr_validate(&address)?,
                unstaking_duration: msg.unstaking_duration,
                nft_code_hash: code_hash.clone(),
                query_auth: msg.query_auth.into_valid(deps.api)?,
            };
            CONFIG.save(deps.storage, &config)?;

            Ok(Response::default()
                .add_attribute("method", "instantiate")
                .add_attribute("nft_contract", address))
        }
        NftContract::New {
            code_id,
            code_hash,
            label,
            msg: instantiate_msg,
            initial_nfts,
        } => {
            // Deserialize the binary msg into cw721
            let mut instantiate_msg = try_deserialize_nft_instantiate_msg(instantiate_msg)?;

            // Modify the InstantiateMsg such that the minter is now this contract.
            // We will update ownership of the NFT contract to be the DAO in the submessage reply.
            instantiate_msg.modify_instantiate_msg(env.contract.address.as_str());

            // Check there is at least one NFT to initialize
            if initial_nfts.is_empty() {
                return Err(ContractError::NoInitialNfts {});
            }

            // Save config with empty nft_address
            let config = Config {
                nft_address: Addr::unchecked(""),
                unstaking_duration: msg.unstaking_duration,
                nft_code_hash: code_hash.clone(),
                query_auth: msg.query_auth.into_valid(deps.api)?,
            };
            CONFIG.save(deps.storage, &config)?;

            // Save initial NFTs for use in reply
            INITIAL_NFTS.save(deps.storage, &initial_nfts)?;

            // Create instantiate submessage for NFT contract
            let instantiate_msg = SubMsg::reply_always(
                instantiate_msg.to_cosmos_msg(
                    Some(info.sender.into_string().clone()),
                    label,
                    code_id,
                    code_hash.clone(),
                    None,
                )?,
                INSTANTIATE_NFT_CONTRACT_REPLY_ID,
            );

            Ok(Response::default()
                .add_attribute("method", "instantiate")
                .add_submessage(instantiate_msg))
        }
        // This is unimplemented as submsg implementation works differently in secret network
        NftContract::Factory(binary) => match from_binary(&binary)? {
            WasmMsg::Execute {
                msg: wasm_msg,
                code_hash,
                contract_addr,
                funds,
            } => {
                // Save config with empty nft_address
                let config = Config {
                    nft_address: Addr::unchecked(""),
                    unstaking_duration: msg.unstaking_duration,
                    nft_code_hash: code_hash.clone(),
                    query_auth: msg.query_auth.into_valid(deps.api)?,
                };
                CONFIG.save(deps.storage, &config)?;

                // Call factory contract. Use only a trusted factory contract,
                // as this is a critical security component and valdiation of
                // setup will happen in the factory.
                Ok(Response::new()
                    .add_attribute("action", "intantiate")
                    .add_submessage(SubMsg::reply_on_success(
                        WasmMsg::Execute {
                            contract_addr,
                            code_hash: code_hash.clone(),
                            msg: wasm_msg,
                            funds,
                        },
                        FACTORY_EXECUTE_REPLY_ID,
                    )))
            }
            _ => Err(ContractError::UnsupportedFactoryMsg {}),
        },
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<Empty>, ContractError> {
    match msg {
        ExecuteMsg::ReceiveNft(msg) => execute_stake(deps, env, info, msg),
        ExecuteMsg::Unstake { token_ids } => execute_unstake(deps, env, info, token_ids),
        ExecuteMsg::ClaimNfts {} => execute_claim_nfts(deps, env, info),
        ExecuteMsg::UpdateConfig { duration } => execute_update_config(info, deps, duration),
        ExecuteMsg::AddHook { addr, code_hash } => execute_add_hook(deps, info, addr, code_hash),
        ExecuteMsg::RemoveHook { addr, code_hash } => {
            execute_remove_hook(deps, info, addr, code_hash)
        }
        ExecuteMsg::UpdateActiveThreshold { new_threshold } => {
            execute_update_active_threshold(deps, env, info, new_threshold)
        }
    }
}

pub fn execute_stake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    wrapper: Snip721ReceiveMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.nft_address {
        return Err(ContractError::InvalidToken {
            received: info.sender,
            expected: config.nft_address,
        });
    }
    match wrapper {
        Snip721ReceiveMsg::ReceiveNft {
            sender,
            token_id,
            msg: _,
        } => {
            register_staked_nft(
                deps.storage,
                env.block.height,
                sender.clone(),
                token_id.clone(),
            )?;
            let hook_msgs =
                stake_nft_hook_msgs(HOOKS, deps.storage, sender.clone(), token_id.clone())?;
            Ok(Response::default()
                .add_submessages(hook_msgs)
                .add_attribute("action", "stake")
                .add_attribute("from", sender)
                .add_attribute("token_id", token_id))
        }
        _ => Ok(Response::default()),
    }
}

pub fn execute_unstake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token_ids: Vec<String>,
) -> Result<Response, ContractError> {
    if token_ids.is_empty() {
        return Err(ContractError::ZeroUnstake {});
    }

    register_unstaked_nfts(
        deps.storage,
        env.block.height,
        info.sender.clone(),
        &token_ids,
    )?;

    // Provided that the backing cw721 contract is non-malicious:
    //
    // 1. no token that has been staked may be staked again before
    //    first being unstaked.
    //
    // Provided that the other methods on this contract are functional:
    //
    // 2. there will never exist a pending claim for a token that is
    //    unstaked.
    // 3. (6) => claims may only be created for tokens that are staked.
    // 4. (1) && (2) && (3) => there will never be a staked NFT for
    //    which there is also a pending claim.
    //
    // (aside: the requirement on (1) for (4) may be confusing. it is
    // needed because if a token could be staked more than once, a
    // token could be staked, moved into the claims queue, and then
    // staked again, in which case the token is both staked and has a
    // pending claim.)
    //
    // If we reach this point in execution, `register_unstaked_nfts`
    // has not errored and thus:
    //
    // 5. token_ids contains no duplicate values.
    // 6. all NFTs in token_ids were staked by `info.sender`
    // 7. (4) && (6) => none of the tokens in token_ids are in the
    //    claims queue for `info.sender`
    //
    // (5) && (7) are the invariants for calling `create_nft_claims`
    // so if we reach this point in execution, we may safely create
    // claims.

    let hook_msgs =
        unstake_nft_hook_msgs(HOOKS, deps.storage, info.sender.clone(), token_ids.clone())?;

    let config = CONFIG.load(deps.storage)?;
    match config.unstaking_duration {
        None => {
            let return_messages = token_ids
                .into_iter()
                .map(|token_id| -> StdResult<WasmMsg> {
                    Ok(cosmwasm_std::WasmMsg::Execute {
                        contract_addr: config.nft_address.to_string(),
                        code_hash: config.nft_code_hash.clone(),
                        msg: to_binary(&secret_toolkit::snip721::HandleMsg::TransferNft {
                            recipient: info.sender.to_string(),
                            token_id,
                            memo: None,
                            padding: None,
                        })?,
                        funds: vec![],
                    })
                })
                .collect::<StdResult<Vec<_>>>()?;

            Ok(Response::default()
                .add_messages(return_messages)
                .add_submessages(hook_msgs)
                .add_attribute("action", "unstake")
                .add_attribute("from", info.sender)
                .add_attribute("claim_duration", "None"))
        }

        Some(duration) => {
            let outstanding_claims = NFT_CLAIMS
                .query_claims(deps.as_ref(), &info.sender)?
                .nft_claims;
            if outstanding_claims.len() + token_ids.len() > MAX_CLAIMS as usize {
                return Err(ContractError::TooManyClaims {});
            }

            // Out of gas here is fine - just try again with fewer
            // tokens.
            NFT_CLAIMS.create_nft_claims(
                deps.storage,
                &info.sender,
                token_ids,
                duration.after(&env.block),
            )?;

            Ok(Response::default()
                .add_attribute("action", "unstake")
                .add_submessages(hook_msgs)
                .add_attribute("from", info.sender)
                .add_attribute("claim_duration", format!("{duration}")))
        }
    }
}

pub fn execute_claim_nfts(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let nfts = NFT_CLAIMS.claim_nfts(deps.storage, &info.sender, &env.block)?;
    if nfts.is_empty() {
        return Err(ContractError::NothingToClaim {});
    }

    let config = CONFIG.load(deps.storage)?;

    let msgs = nfts
        .into_iter()
        .map(|nft| -> StdResult<CosmosMsg> {
            Ok(WasmMsg::Execute {
                contract_addr: config.nft_address.to_string(),
                code_hash: config.nft_code_hash.clone(),
                msg: to_binary(&secret_toolkit::snip721::HandleMsg::TransferNft {
                    recipient: info.sender.to_string(),
                    token_id: nft,
                    memo: None,
                    padding: None,
                })?,
                funds: vec![],
            }
            .into())
        })
        .collect::<StdResult<Vec<_>>>()?;

    Ok(Response::default()
        .add_messages(msgs)
        .add_attribute("action", "claim_nfts")
        .add_attribute("from", info.sender))
}

pub fn execute_update_config(
    info: MessageInfo,
    deps: DepsMut,
    duration: Option<Duration>,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;
    let dao = DAO.load(deps.storage)?;

    // Only the DAO can update the config.
    if info.sender != dao.addr {
        return Err(ContractError::Unauthorized {});
    }

    // Validate unstaking duration
    validate_duration(duration)?;

    config.unstaking_duration = duration;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default()
        .add_attribute("action", "update_config")
        .add_attribute(
            "unstaking_duration",
            config
                .unstaking_duration
                .map(|d| d.to_string())
                .unwrap_or_else(|| "none".to_string()),
        ))
}

pub fn execute_add_hook(
    deps: DepsMut,
    info: MessageInfo,
    addr: String,
    code_hash: String,
) -> Result<Response, ContractError> {
    let dao = DAO.load(deps.storage)?;

    // Only the DAO can add a hook
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

    Ok(Response::default()
        .add_attribute("action", "add_hook")
        .add_attribute("hook", addr))
}

pub fn execute_remove_hook(
    deps: DepsMut,
    info: MessageInfo,
    addr: String,
    code_hash: String,
) -> Result<Response, ContractError> {
    let dao = DAO.load(deps.storage)?;

    // Only the DAO can remove a hook
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

    Ok(Response::default()
        .add_attribute("action", "remove_hook")
        .add_attribute("hook", addr))
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

    let config = CONFIG.load(deps.storage)?;
    if let Some(active_threshold) = new_active_threshold {
        match active_threshold {
            ActiveThreshold::Percentage { percent } => {
                assert_valid_percentage_threshold(percent)?;
            }
            ActiveThreshold::AbsoluteCount { count } => {
                let nft_supply: secret_toolkit::snip721::query::NumTokens =
                    deps.querier.query_wasm_smart(
                        config.nft_code_hash.clone(),
                        config.nft_address,
                        &secret_toolkit::snip721::QueryMsg::NumTokens { viewer: None },
                    )?;
                assert_valid_absolute_count_threshold(
                    count,
                    Uint128::new(nft_supply.count.into()),
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
        QueryMsg::ActiveThreshold {} => query_active_threshold(deps),
        QueryMsg::Config {} => query_config(deps),
        QueryMsg::Dao {} => query_dao(deps),
        QueryMsg::Info {} => query_info(deps),
        QueryMsg::IsActive {} => query_is_active(deps, env),
        QueryMsg::Hooks {} => query_hooks(deps),
        QueryMsg::TotalPowerAtHeight { height } => query_total_power_at_height(deps, env, height),
        QueryMsg::VotingPowerAtHeight { auth, height } => {
            let query_auth = CONFIG.load(deps.storage)?.query_auth;
            let user = authenticate(deps, auth, query_auth)?;
            query_voting_power_at_height(deps, env, user, height)
        }
        QueryMsg::NftClaims { auth } => {
            let query_auth = CONFIG.load(deps.storage)?.query_auth;
            let user = authenticate(deps, auth, query_auth)?;
            query_nft_claims(deps, user)
        }
        QueryMsg::StakedNfts { auth } => {
            let query_auth = CONFIG.load(deps.storage)?.query_auth;
            let user = authenticate(deps, auth, query_auth)?;
            query_staked_nfts(deps, user)
        }
    }
}

pub fn query_active_threshold(deps: Deps) -> StdResult<Binary> {
    to_binary(&ActiveThresholdResponse {
        active_threshold: ACTIVE_THRESHOLD.may_load(deps.storage)?,
    })
}

pub fn query_is_active(deps: Deps, env: Env) -> StdResult<Binary> {
    let threshold = ACTIVE_THRESHOLD.may_load(deps.storage)?;
    if let Some(threshold) = threshold {
        let config = CONFIG.load(deps.storage)?;
        let staked_nfts = StakedNftsTotalStore::may_load_at_height(deps.storage, env.block.height)?;
        let total_nfts: secret_toolkit::snip721::query::NumTokens = deps.querier.query_wasm_smart(
            config.nft_code_hash.clone(),
            config.nft_address,
            &secret_toolkit::snip721::QueryMsg::NumTokens { viewer: None },
        )?;

        match threshold {
            ActiveThreshold::AbsoluteCount { count } => to_binary(&IsActiveResponse {
                active: staked_nfts.unwrap() >= count,
            }),
            ActiveThreshold::Percentage { percent } => {
                // Check if there are any staked NFTs
                if staked_nfts.unwrap().is_zero() {
                    return to_binary(&IsActiveResponse { active: false });
                }

                // percent is bounded between [0, 100]. decimal
                // represents percents in u128 terms as p *
                // 10^15. this bounds percent between [0, 10^17].
                //
                // total_potential_power is bounded between [0, 2^64]
                // as it tracks the count of NFT tokens which has
                // a max supply of 2^64.
                //
                // with our precision factor being 10^9:
                //
                // total_nfts <= 2^64 * 10^9 <= 2^256
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
                let total_nfts_count = Uint128::from(total_nfts.count).full_mul(PRECISION_FACTOR);

                // under the hood decimals are `atomics / 10^decimal_places`.
                // cosmwasm doesn't give us a Decimal * Uint256
                // implementation so we take the decimal apart and
                // multiply by the fraction.
                let applied = total_nfts_count.multiply_ratio(
                    percent.atomics(),
                    Uint256::from(10u64).pow(percent.decimal_places()),
                );
                let rounded = (applied + Uint256::from(PRECISION_FACTOR) - Uint256::from(1u128))
                    / Uint256::from(PRECISION_FACTOR);
                let count: Uint128 = rounded.try_into().unwrap();

                // staked_nfts >= total_nfts * percent
                to_binary(&IsActiveResponse {
                    active: staked_nfts.unwrap() >= count,
                })
            }
        }
    } else {
        to_binary(&IsActiveResponse { active: true })
    }
}

pub fn query_voting_power_at_height(
    deps: Deps,
    env: Env,
    address: Addr,
    height: Option<u64>,
) -> StdResult<Binary> {
    let height = height.unwrap_or(env.block.height);
    let power = NftBalancesStore::may_load_at_height(deps.storage, address, height)?;
    to_binary(&dao_interface::voting::VotingPowerAtHeightResponse {
        power: power.unwrap(),
        height,
    })
}

pub fn query_total_power_at_height(deps: Deps, env: Env, height: Option<u64>) -> StdResult<Binary> {
    let height = height.unwrap_or(env.block.height);
    let power = StakedNftsTotalStore::may_load_at_height(deps.storage, height)?;
    to_binary(&dao_interface::voting::TotalPowerAtHeightResponse {
        power: power.unwrap(),
        height,
    })
}

pub fn query_config(deps: Deps) -> StdResult<Binary> {
    let config = CONFIG.load(deps.storage)?;
    to_binary(&config)
}

pub fn query_dao(deps: Deps) -> StdResult<Binary> {
    let dao = DAO.load(deps.storage)?;
    to_binary(&dao)
}

pub fn query_nft_claims(deps: Deps, address: Addr) -> StdResult<Binary> {
    to_binary(&NFT_CLAIMS.query_claims(deps, &address)?)
}

pub fn query_hooks(deps: Deps) -> StdResult<Binary> {
    to_binary(&HOOKS.query_hooks(deps)?)
}

pub fn query_info(deps: Deps) -> StdResult<Binary> {
    let info = secret_cw2::get_contract_version(deps.storage)?;
    to_binary(&dao_interface::voting::InfoResponse { info })
}

pub fn query_staked_nfts(deps: Deps, address: Addr) -> StdResult<Binary> {
    // let prefix = deps.api.addr_validate(&address)?;
    // let res = STAKED_NFTS_PER_OWNER.prefix(&prefix);

    // let start_after = start_after.as_deref().map(Bound::exclusive);
    // let range = prefix.keys(
    //     deps.storage,
    //     start_after,
    //     None,
    //     cosmwasm_std::Order::Ascending,
    // );
    // let range: StdResult<Vec<String>> = match limit {
    //     Some(l) => range.take(l as usize).collect(),
    //     None => range.collect(),
    // };

    let res = NftBalancesStore::load(deps.storage, address);

    to_binary(&res)
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
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_NFT_CONTRACT_REPLY_ID => {
            match msg.result {
                SubMsgResult::Ok(res) => {
                    let dao = DAO.load(deps.storage)?;
                    let nft_contract_address = parse_reply_event_for_contract_address(res.events)?;

                    // Save NFT contract to config
                    let mut config = CONFIG.load(deps.storage)?;
                    config.nft_address = deps.api.addr_validate(&nft_contract_address.clone())?;
                    CONFIG.save(deps.storage, &config)?;

                    let initial_nfts = INITIAL_NFTS.load(deps.storage)?;

                    // Add mint submessages
                    let mut submessages: Vec<SubMsg> = initial_nfts
                        .iter()
                        .flat_map(|nft| -> Result<SubMsg, ContractError> {
                            Ok(SubMsg::new(WasmMsg::Execute {
                                contract_addr: nft_contract_address.clone(),
                                funds: vec![],
                                msg: nft.clone(),
                                code_hash: config.nft_code_hash.clone(),
                            }))
                        })
                        .collect::<Vec<SubMsg>>();

                    // Clear space
                    INITIAL_NFTS.remove(deps.storage);

                    // The last submessage updates the minter / owner of the NFT contract,
                    // and triggers a reply. The reply is used for validation after setup.
                    let exec_msg = snip721::Snip721ExecuteMsg::ChangeAdmin {
                        address: dao.addr.to_string(),
                        padding: None,
                    };
                    submessages.push(SubMsg::reply_on_success(
                        exec_msg.to_cosmos_msg(
                            config.nft_code_hash.clone(),
                            nft_contract_address.clone(),
                            None,
                        )?,
                        VALIDATE_SUPPLY_REPLY_ID,
                    ));

                    Ok(Response::default()
                        .add_attribute("nft_contract", nft_contract_address.clone())
                        .add_submessages(submessages))
                }
                SubMsgResult::Err(_) => Err(ContractError::NftInstantiateError {}),
            }
        }
        VALIDATE_SUPPLY_REPLY_ID => {
            // Check that NFTs have actually been minted, and that supply is greater than zero
            // NOTE: we have to check this in a reply as it is potentially possible
            // to include non-mint messages in `initial_nfts`.

            // Load config for nft contract address
            let collection_addr = CONFIG.load(deps.storage)?.nft_address;
            let collection_code_hash = CONFIG.load(deps.storage)?.nft_code_hash;

            // Query the total supply of the NFT contract
            let nft_supply: secret_toolkit::snip721::query::NumTokens =
                deps.querier.query_wasm_smart(
                    collection_code_hash.clone(),
                    collection_addr.clone(),
                    &secret_toolkit::snip721::QueryMsg::NumTokens { viewer: None },
                )?;

            // Check greater than zero
            if nft_supply.count == 0 {
                return Err(ContractError::NoInitialNfts {});
            }

            // If Active Threshold absolute count is configured,
            // check the count is not greater than supply
            if let Some(ActiveThreshold::AbsoluteCount { count }) =
                ACTIVE_THRESHOLD.may_load(deps.storage)?
            {
                assert_valid_absolute_count_threshold(
                    count,
                    Uint128::new(nft_supply.count.into()),
                )?;
            }

            // On setup success, have the DAO complete the second part of
            // ownership transfer by accepting ownership in a
            // ModuleInstantiateCallback.

            // NOTE Can't find how to implement in secret so used this

            let callback = to_binary(&ModuleInstantiateCallback {
                msgs: vec![CosmosMsg::Wasm(WasmMsg::Execute {
                    code_hash: collection_code_hash.clone(),
                    contract_addr: collection_addr.to_string(),
                    msg: to_binary(&&snip721_reference_impl::msg::ExecuteMsg::ChangeAdmin {
                        address: DAO.load(deps.storage)?.addr.to_string(),
                        padding: None,
                    })?,
                    funds: vec![],
                })],
            })?;

            Ok(Response::new().set_data(callback))

            // Ok(Response::new())
        }
        FACTORY_EXECUTE_REPLY_ID => {
            // Parse reply data
            match msg.result {
                SubMsgResult::Ok(data) => {
                    let mut config = CONFIG.load(deps.storage)?;

                    // Parse info from the callback, this will fail
                    // if incorrectly formatted.
                    let info: NftFactoryCallback = from_binary(&data.data.unwrap())?;

                    // Validate NFT contract address
                    let nft_address = deps.api.addr_validate(&info.nft_contract)?;

                    // Validate that this is an NFT with a query
                    deps.querier
                        .query_wasm_smart::<secret_toolkit::snip721::query::NumTokens>(
                            info.nft_code_hash.clone(),
                            nft_address.clone(),
                            &secret_toolkit::snip721::QueryMsg::NumTokens { viewer: None },
                        )?;

                    // Update NFT contract
                    config.nft_address = nft_address;
                    CONFIG.save(deps.storage, &config)?;

                    // Construct the response
                    let mut res = Response::new().add_attribute("nft_contract", info.nft_contract);

                    // If a callback has been configured, set the module
                    // instantiate callback data.
                    if let Some(callback) = info.module_instantiate_callback {
                        res = res.set_data(to_binary(&callback)?);
                    }

                    Ok(res)
                }
                SubMsgResult::Err(_) => Err(ContractError::UnknownReplyId { id: msg.id }),
            }
        }
        _ => Err(ContractError::UnknownReplyId { id: msg.id }),
    }
}
