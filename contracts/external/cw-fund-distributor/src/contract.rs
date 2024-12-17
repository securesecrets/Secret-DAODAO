use crate::error::ContractError;
use crate::msg::{
    DenomResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, NativeEntitlementResponse, QueryMsg,
    Snip20EntitlementResponse, Snip20ReceiveMsg, Snip20Response, TokenInfo, TotalPowerResponse,
    VotingContractResponse,
};
use crate::state::{
    Config, VotingContractInfo, CONFIG, DISTRIBUTION_HEIGHT, FUNDING_PERIOD_EXPIRATION,
    NATIVE_BALANCES, NATIVE_CLAIMS, SNIP20S_CODE_HASH, SNIP20_BALANCES, SNIP20_CLAIMS, TOTAL_POWER,
    VOTING_CONTRACT,
};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, BankMsg, Binary, Coin, Decimal, Deps, DepsMut, Env, Fraction, MessageInfo,
    Response, StdError, StdResult, Uint128, WasmMsg,
};
use secret_cw2::set_contract_version;

use dao_interface::voting;
use secret_toolkit::storage::keymap::KeyIter;
use secret_toolkit::storage::Keymap;
use serde::de::DeserializeOwned;
use serde::Serialize;
use shade_protocol::basic_staking::{Auth, AuthPermit};
use shade_protocol::query_auth::helpers::{
    authenticate_permit, authenticate_vk, PermitAuthentication,
};
use shade_protocol::Contract;

const CONTRACT_NAME: &str = "crates.io:cw-fund-distributor";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

type NativeClaimEntry = Result<((Addr, String), Uint128), StdError>;
type Cw20ClaimEntry = Result<((Addr, Addr), Uint128), StdError>;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // store the height
    DISTRIBUTION_HEIGHT.save(deps.storage, &msg.distribution_height)?;

    // get the funding expiration and store it
    let funding_expiration_height = msg.funding_period.after(&env.block);
    FUNDING_PERIOD_EXPIRATION.save(deps.storage, &funding_expiration_height)?;

    // validate the contract and save it
    let voting_contract = deps.api.addr_validate(&msg.voting_contract)?;
    VOTING_CONTRACT.save(
        deps.storage,
        &VotingContractInfo {
            address: voting_contract.clone(),
            code_hash: msg.voting_contract_hash.clone(),
        },
    )?;

    let total_power: voting::TotalPowerAtHeightResponse = deps.querier.query_wasm_smart(
        msg.voting_contract_hash,
        voting_contract.clone(),
        &voting::Query::TotalPowerAtHeight {
            height: Some(env.block.height),
        },
    )?;
    // validate the total power and store it
    if total_power.power.is_zero() {
        return Err(ContractError::ZeroVotingPower {});
    }
    TOTAL_POWER.save(deps.storage, &total_power.power)?;
    CONFIG.save(
        deps.storage,
        &Config {
            owner: info.sender,
            query_auth: msg.query_auth.into_valid(deps.api).unwrap_or_default(),
        },
    )?;

    Ok(Response::default()
        .add_attribute("distribution_height", env.block.height.to_string())
        .add_attribute("voting_contract", voting_contract)
        .add_attribute("total_power", total_power.power))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(Snip20ReceiveMsg {
            sender: _,
            amount,
            msg: _,
            from: _,
            memo: _,
        }) => execute_fund_snip20(deps, env, info.sender, amount),
        ExecuteMsg::FundNative {} => execute_fund_native(deps, env, info),
        ExecuteMsg::ClaimSnip20 { auth, tokens } => {
            execute_claim_snip20s(deps, env, info.sender, auth, tokens)
        }
        ExecuteMsg::ClaimNatives { auth, denoms } => {
            execute_claim_natives(deps, env, auth, info.sender, denoms)
        }
        ExecuteMsg::ClaimAll { auth } => execute_claim_all(deps, env, info.sender, auth),
        ExecuteMsg::SetSnip20sCodeHash { token_info } => {
            if info.sender != CONFIG.load(deps.storage)?.owner {
                return Err(ContractError::Unauthorized {});
            }
            for token_info in token_info {
                SNIP20S_CODE_HASH.insert(
                    deps.storage,
                    &token_info.address,
                    &token_info.code_hash,
                )?;
            }
            Ok(Response::default())
        }
    }
}

pub fn execute_fund_snip20(
    deps: DepsMut,
    env: Env,
    token: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let funding_deadline = FUNDING_PERIOD_EXPIRATION.load(deps.storage)?;
    // if current block indicates claiming period, return an error
    if funding_deadline.is_expired(&env.block) {
        return Err(ContractError::FundDuringClaimingPeriod {});
    }

    if amount > Uint128::zero() {
        match SNIP20_BALANCES.get(deps.storage, &token.clone()) {
            // If the token balance exists, update it
            Some(old_amount) => {
                let new_amount = old_amount.checked_add(amount)?;
                SNIP20_BALANCES.insert(deps.storage, &token, &new_amount)?;
            }
            // If the token balance doesn't exist, insert a new balance
            None => {
                SNIP20_BALANCES.insert(deps.storage, &token, &amount)?;
            }
        }
    }

    Ok(Response::default()
        .add_attribute("method", "fund_snip20")
        .add_attribute("token", token)
        .add_attribute("amount", amount))
}

pub fn execute_fund_native(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let funding_deadline = FUNDING_PERIOD_EXPIRATION.load(deps.storage)?;
    // if current block indicates claiming period, return an error
    if funding_deadline.is_expired(&env.block) {
        return Err(ContractError::FundDuringClaimingPeriod {});
    }

    // collect a list of successful funding kv pairs
    let mut attributes: Vec<(String, String)> = Vec::new();

    for coin in info.funds {
        if coin.amount > Uint128::zero() {
            let current_balance = NATIVE_BALANCES.get(deps.storage, &coin.denom.clone());

            let new_balance = match current_balance {
                Some(current_balance) => coin.amount.checked_add(current_balance)?,
                None => coin.amount,
            };

            NATIVE_BALANCES.insert(deps.storage, &coin.denom, &new_balance)?;

            attributes.push((coin.denom.clone(), coin.amount.to_string()));
        }
    }

    Ok(Response::default()
        .add_attribute("method", "fund_native")
        .add_attributes(attributes))
}

fn get_entitlement(
    distributor_funds: Uint128,
    relative_share: Decimal,
    previous_claim: Uint128,
) -> Result<Uint128, ContractError> {
    let total_share =
        distributor_funds.multiply_ratio(relative_share.numerator(), relative_share.denominator());
    match total_share.checked_sub(previous_claim) {
        Ok(entitlement) => Ok(entitlement),
        Err(e) => Err(ContractError::OverflowErr(e)),
    }
}

fn get_relative_share(deps: &Deps, auth: Auth) -> Result<Decimal, StdError> {
    let voting_contract_info = VOTING_CONTRACT.load(deps.storage)?;
    let dist_height = DISTRIBUTION_HEIGHT.load(deps.storage)?;
    let total_power = TOTAL_POWER.load(deps.storage)?;

    // find the voting power of sender at distributor instantiation
    let voting_power: voting::VotingPowerAtHeightResponse = deps.querier.query_wasm_smart(
        voting_contract_info.code_hash,
        voting_contract_info.address.to_string(),
        &voting::Query::VotingPowerAtHeight {
            auth,
            height: Some(dist_height),
        },
    )?;
    // return senders share
    Ok(Decimal::from_ratio(voting_power.power, total_power))
}

pub fn execute_claim_snip20s(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    auth: Auth,
    tokens: Vec<TokenInfo>,
) -> Result<Response, ContractError> {
    let funding_deadline = FUNDING_PERIOD_EXPIRATION.load(deps.storage)?;
    // if current block indicates funding period, return an error
    if !funding_deadline.is_expired(&env.block) {
        return Err(ContractError::ClaimDuringFundingPeriod {});
    }
    if tokens.is_empty() {
        return Err(ContractError::EmptyClaim {});
    }

    let relative_share = get_relative_share(&deps.as_ref(), auth.clone())?;
    let messages = get_snip20_claim_wasm_messages(tokens, deps, sender.clone(), relative_share)?;

    Ok(Response::default()
        .add_attribute("method", "claim_cw20s")
        .add_attribute("sender", sender)
        .add_messages(messages))
}

/// Looks at the SNIP20_BALANCES map entries and returns a vector of WasmMsg::Execute
/// messages that entail the amount that the user is entitled to.
/// Updates the SNIP20_CLAIMS entries accordingly.
fn get_snip20_claim_wasm_messages(
    tokens: Vec<TokenInfo>,
    deps: DepsMut,
    sender: Addr,
    relative_share: Decimal,
) -> Result<Vec<WasmMsg>, ContractError> {
    let mut messages: Vec<WasmMsg> = vec![];
    for token_info in tokens {
        // get the balance of distributor at instantiation
        let bal = SNIP20_BALANCES
            .get(deps.storage, &Addr::unchecked(token_info.address.clone()))
            .unwrap_or_default();

        // check for any previous claims
        let previous_claim = SNIP20_CLAIMS
            .get(
                deps.storage,
                &(sender.clone(), Addr::unchecked(token_info.address.clone())),
            )
            .unwrap_or_default();

        // get % share of sender and subtract any previous claims
        let entitlement = get_entitlement(bal, relative_share, previous_claim)?;
        if !entitlement.is_zero() {
            // reflect the new total claim amount
            let previous_claim = SNIP20_CLAIMS.get(
                deps.storage,
                &(sender.clone(), Addr::unchecked(token_info.address.clone())),
            );

            let new_claim = match previous_claim {
                Some(previous_claim) => previous_claim
                    .checked_add(entitlement)
                    .map_err(ContractError::OverflowErr),
                None => Ok(entitlement),
            };

            SNIP20_CLAIMS.insert(
                deps.storage,
                &(sender.clone(), Addr::unchecked(token_info.address.clone())),
                &new_claim?,
            )?;

            messages.push(WasmMsg::Execute {
                contract_addr: token_info.address.to_string(),
                code_hash: token_info.code_hash,
                msg: to_binary(&snip20_base::msg::ExecuteMsg::Transfer {
                    recipient: sender.to_string(),
                    amount: entitlement,
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })?,
                funds: vec![],
            });
        }
    }

    Ok(messages)
}

pub fn execute_claim_natives(
    deps: DepsMut,
    env: Env,
    auth: Auth,
    sender: Addr,
    denoms: Vec<String>,
) -> Result<Response, ContractError> {
    let funding_deadline = FUNDING_PERIOD_EXPIRATION.load(deps.storage)?;
    // if current block indicates funding period, return an error
    if !funding_deadline.is_expired(&env.block) {
        return Err(ContractError::ClaimDuringFundingPeriod {});
    }
    if denoms.is_empty() {
        return Err(ContractError::EmptyClaim {});
    }

    // find the relative share of the distributor pool for the user
    // and determine the native claim transfer amounts with it
    let relative_share = get_relative_share(&deps.as_ref(), auth.clone())?;
    let messages = get_native_claim_bank_messages(denoms, deps, sender.clone(), relative_share)?;

    Ok(Response::default()
        .add_attribute("method", "claim_natives")
        .add_attribute("sender", sender)
        .add_messages(messages))
}

/// Looks at the NATIVE_BALANCES map entries and returns a vector of
/// BankMsg::Send messages that entail the amount that the user is
/// entitled to. Updates the NATIVE_CLAIMS entries accordingly.
fn get_native_claim_bank_messages(
    denoms: Vec<String>,
    deps: DepsMut,
    sender: Addr,
    relative_share: Decimal,
) -> Result<Vec<BankMsg>, ContractError> {
    let mut messages: Vec<BankMsg> = vec![];

    for addr in denoms {
        // get the balance of distributor at instantiation
        let bal = NATIVE_BALANCES
            .get(deps.storage, &addr.clone())
            .unwrap_or_default();

        // check for any previous claims
        let previous_claim = NATIVE_CLAIMS
            .get(deps.storage, &(sender.clone(), addr.clone()))
            .unwrap_or_default();

        // get % share of sender and subtract any previous claims
        let entitlement = get_entitlement(bal, relative_share, previous_claim)?;
        if !entitlement.is_zero() {
            // reflect the new total claim amount
            let previous_claim = NATIVE_CLAIMS.get(deps.storage, &(sender.clone(), addr.clone()));

            let new_claim = match previous_claim {
                Some(previous_claim) => previous_claim
                    .checked_add(entitlement)
                    .map_err(ContractError::OverflowErr),
                None => Ok(entitlement),
            };

            NATIVE_CLAIMS.insert(deps.storage, &(sender.clone(), addr.clone()), &new_claim?)?;

            // collect the transfer messages
            messages.push(BankMsg::Send {
                to_address: sender.to_string(),
                amount: vec![Coin {
                    denom: addr,
                    amount: entitlement,
                }],
            });
        }
    }
    Ok(messages)
}

pub fn execute_claim_all(
    mut deps: DepsMut,
    env: Env,
    sender: Addr,
    auth: Auth,
) -> Result<Response, ContractError> {
    let funding_deadline = FUNDING_PERIOD_EXPIRATION.load(deps.storage)?;
    // claims cannot happen during funding period
    if !funding_deadline.is_expired(&env.block) {
        return Err(ContractError::ClaimDuringFundingPeriod {});
    }

    // get the lists of tokens in distributor pool
    let snip20s: Vec<Result<Addr, _>> = SNIP20_BALANCES.iter_keys(deps.storage)?.collect();

    let mut snip20_info: Vec<TokenInfo> = vec![];
    for entry in snip20s {
        let addr = entry?;
        let code_hash = SNIP20S_CODE_HASH.get(deps.storage, &addr.clone()).unwrap();
        let token_info = TokenInfo {
            address: addr,
            code_hash: code_hash.clone(),
        };
        snip20_info.push(token_info);
    }

    let native_denoms: Vec<Result<String, _>> = NATIVE_BALANCES.iter_keys(deps.storage)?.collect();
    let mut denoms = vec![];
    for denom in native_denoms {
        denoms.push(denom?);
    }

    let relative_share = get_relative_share(&deps.as_ref(), auth.clone())?;

    // get the claim messages
    let cw20_claim_msgs =
        get_snip20_claim_wasm_messages(snip20_info, deps.branch(), sender.clone(), relative_share)?;
    let native_claim_msgs =
        get_native_claim_bank_messages(denoms, deps.branch(), sender, relative_share)?;

    Ok(Response::default()
        .add_attribute("method", "claim_all")
        .add_messages(cw20_claim_msgs)
        .add_messages(native_claim_msgs))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::VotingContract {} => query_voting_contract(deps),
        QueryMsg::TotalPower {} => query_total_power(deps),
        QueryMsg::NativeDenoms {} => query_native_denoms(deps),
        QueryMsg::Snip20Tokens {} => query_snip20_tokens(deps),
        QueryMsg::NativeEntitlement { auth, denom } => {
            let query_auth = CONFIG.load(deps.storage)?.query_auth;
            let sender = authenticate(deps, auth.clone(), query_auth)?;
            query_native_entitlement(deps, auth, sender, denom)
        }
        QueryMsg::Snip20Entitlement { auth, token } => {
            let query_auth = CONFIG.load(deps.storage)?.query_auth;
            let sender = authenticate(deps, auth.clone(), query_auth)?;
            query_snip20_entitlement(deps, auth, sender, token)
        }
        QueryMsg::NativeEntitlements {
            auth,
            start_at,
            limit,
        } => {
            let query_auth = CONFIG.load(deps.storage)?.query_auth;
            let sender = authenticate(deps, auth.clone(), query_auth)?;
            query_native_entitlements(deps, auth, sender, start_at, limit)
        }
        QueryMsg::Snip20Entitlements {
            auth,
            start_at,
            limit,
        } => {
            let query_auth = CONFIG.load(deps.storage)?.query_auth;
            let sender = authenticate(deps, auth.clone(), query_auth)?;
            query_snip20_entitlements(deps, auth, sender, start_at, limit)
        }
    }
}

pub fn query_voting_contract(deps: Deps) -> StdResult<Binary> {
    let contract = VOTING_CONTRACT.load(deps.storage)?;
    let distribution_height = DISTRIBUTION_HEIGHT.load(deps.storage)?;
    to_binary(&VotingContractResponse {
        contract,
        distribution_height,
    })
}

pub fn query_total_power(deps: Deps) -> StdResult<Binary> {
    let total_power: Uint128 = TOTAL_POWER.may_load(deps.storage)?.unwrap_or_default();
    to_binary(&TotalPowerResponse { total_power })
}

pub fn query_native_denoms(deps: Deps) -> StdResult<Binary> {
    let native_balances = NATIVE_BALANCES.iter(deps.storage)?;

    let mut denom_responses: Vec<DenomResponse> = vec![];
    for entry in native_balances {
        let (denom, amount) = entry?;
        denom_responses.push(DenomResponse {
            contract_balance: amount,
            denom,
        });
    }

    to_binary(&denom_responses)
}

pub fn query_snip20_tokens(deps: Deps) -> StdResult<Binary> {
    let cw20_balances = SNIP20_BALANCES.iter(deps.storage)?;

    let mut cw20_responses: Vec<Snip20Response> = vec![];
    for cw20 in cw20_balances {
        let (token, amount) = cw20?;
        cw20_responses.push(Snip20Response {
            contract_balance: amount,
            token: token.to_string(),
        });
    }

    to_binary(&cw20_responses)
}

pub fn query_native_entitlement(
    deps: Deps,
    auth: Auth,
    sender: Addr,
    denom: String,
) -> StdResult<Binary> {
    let prev_claim = NATIVE_CLAIMS
        .get(deps.storage, &(sender.clone(), denom.clone()))
        .unwrap_or_default();
    let total_bal = NATIVE_BALANCES
        .get(deps.storage, &denom.clone())
        .unwrap_or_default();
    let relative_share = get_relative_share(&deps, auth)?;

    let total_share =
        total_bal.multiply_ratio(relative_share.numerator(), relative_share.denominator());
    let entitlement = total_share.checked_sub(prev_claim)?;

    to_binary(&NativeEntitlementResponse {
        amount: entitlement,
        denom,
    })
}

pub fn query_snip20_entitlement(
    deps: Deps,
    auth: Auth,
    sender: Addr,
    token: String,
) -> StdResult<Binary> {
    let token = Addr::unchecked(token);

    let prev_claim = SNIP20_CLAIMS
        .get(deps.storage, &(sender.clone(), token.clone()))
        .unwrap_or_default();
    let total_bal = SNIP20_BALANCES
        .get(deps.storage, &token.clone())
        .unwrap_or_default();
    let relative_share = get_relative_share(&deps, auth)?;

    let total_share =
        total_bal.multiply_ratio(relative_share.numerator(), relative_share.denominator());
    let entitlement = total_share.checked_sub(prev_claim)?;

    to_binary(&Snip20EntitlementResponse {
        amount: entitlement,
        token_contract: token,
    })
}

pub fn query_native_entitlements(
    deps: Deps,
    auth: Auth,
    sender: Addr,
    start_at: Option<String>,
    limit: Option<u32>,
) -> StdResult<Binary> {
    let relative_share = get_relative_share(&deps, auth)?;
    let mut start = start_at.clone(); // Clone start_after to mutate it if necessary
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    // let natives = paginate_map(deps, &NATIVE_BALANCES, start_at, limit, Order::Descending)?;
    let mut natives: Vec<(String, Uint128)> = Vec::new();

    let binding = &NATIVE_BALANCES;
    let iter = binding.iter(deps.storage)?;
    for item in iter {
        let (address, balance) = item?;
        if let Some(start_at) = &start {
            if &address == start_at {
                // If we found the start point, reset it to start iterating
                start = None;
            }
        }
        if start.is_none() {
            natives.push((address, balance));
            if natives.len() >= limit {
                break; // Break out of loop if limit reached
            }
        }
    }

    let mut entitlements: Vec<NativeEntitlementResponse> = vec![];
    for (denom, amount) in natives {
        let prev_claim = NATIVE_CLAIMS
            .get(deps.storage, &(sender.clone(), denom.clone()))
            .unwrap_or_default();
        let total_share =
            amount.multiply_ratio(relative_share.numerator(), relative_share.denominator());
        let entitlement = total_share.checked_sub(prev_claim)?;

        entitlements.push(NativeEntitlementResponse {
            amount: entitlement,
            denom,
        });
    }

    to_binary(&entitlements)
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;
pub fn query_snip20_entitlements(
    deps: Deps,
    auth: Auth,
    sender: Addr,
    start_at: Option<String>,
    limit: Option<u32>,
) -> StdResult<Binary> {
    let relative_share = get_relative_share(&deps, auth)?;
    let mut start = start_at.map(|h| deps.api.addr_validate(&h)).transpose()?;
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let mut snip20s: Vec<(Addr, Uint128)> = Vec::new();

    let binding = &SNIP20_BALANCES;
    let iter = binding.iter(deps.storage)?;
    for item in iter {
        let (address, balance) = item?;
        if let Some(start_at) = &start {
            if &address == start_at {
                // If we found the start point, reset it to start iterating
                start = None;
            }
        }
        if start.is_none() {
            snip20s.push((address, balance));
            if snip20s.len() >= limit {
                break; // Break out of loop if limit reached
            }
        }
    }
    let mut entitlements: Vec<Snip20EntitlementResponse> = vec![];
    for (token, amount) in snip20s {
        let prev_claim = SNIP20_CLAIMS
            .get(deps.storage, &(sender.clone(), token.clone()))
            .unwrap_or_default();

        let total_share =
            amount.multiply_ratio(relative_share.numerator(), relative_share.denominator());
        let entitlement = total_share.checked_sub(prev_claim)?;

        entitlements.push(Snip20EntitlementResponse {
            amount: entitlement,
            token_contract: token,
        });
    }

    to_binary(&entitlements)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    match msg {
        MigrateMsg::RedistributeUnclaimedFunds {
            distribution_height,
        } => execute_redistribute_unclaimed_funds(deps, distribution_height),
    }
}

// only cw_admin can call this
fn execute_redistribute_unclaimed_funds(
    deps: DepsMut,
    distribution_height: u64,
) -> Result<Response, ContractError> {
    // update the distribution height
    DISTRIBUTION_HEIGHT.save(deps.storage, &distribution_height)?;

    // get performed claims of cw20 and native tokens
    let performed_snip20_claims: Vec<Cw20ClaimEntry> = SNIP20_CLAIMS.iter(deps.storage)?.collect();
    let performed_native_claims: Vec<NativeClaimEntry> =
        NATIVE_CLAIMS.iter(deps.storage)?.collect();

    // subtract every performed claim from the available distributor balance
    for entry in performed_snip20_claims {
        let ((_, snip20_addr), amount) = entry?;

        let previous_balance = SNIP20_BALANCES.get(deps.storage, &snip20_addr.clone());

        let new_balance = match previous_balance {
            Some(previous_balance) => previous_balance
                .checked_sub(amount)
                .map_err(ContractError::OverflowErr),
            None => Err(ContractError::Std(StdError::NotFound {
                kind: snip20_addr.to_string(),
            })),
        };
        SNIP20_BALANCES.insert(deps.storage, &snip20_addr.clone(), &new_balance?)?;
    }

    // subtract every performed claim from the available distributor balance
    for entry in performed_native_claims {
        let ((_, denom), amount) = entry?;

        let previous_native_balance = NATIVE_BALANCES.get(deps.storage, &denom.clone());

        let new_native_balance = match previous_native_balance {
            Some(previous_native_balance) => previous_native_balance
                .checked_sub(amount)
                .map_err(ContractError::OverflowErr),
            None => Err(ContractError::Std(StdError::NotFound {
                kind: denom.to_string(),
            })),
        };
        NATIVE_BALANCES.insert(deps.storage, &denom.clone(), &new_native_balance?)?;
    }

    // nullify previous claims
    let snip20_claims = get_keys(deps.as_ref(), &SNIP20_CLAIMS);
    for claims in snip20_claims? {
        let (address, token_address) = claims;
        SNIP20_CLAIMS.remove(deps.storage, &(address, token_address))?;
    }
    let native_claims = get_keys(deps.as_ref(), &NATIVE_CLAIMS);
    for claims in native_claims? {
        let (address, native_denom) = claims;
        NATIVE_CLAIMS.remove(deps.storage, &(address, native_denom))?;
    }

    Ok(Response::default().add_attribute("method", "redistribute_unclaimed_funds"))
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

fn get_keys<K, V>(deps: Deps, map: &Keymap<'_, K, V>) -> StdResult<Vec<K>>
where
    K: Serialize + DeserializeOwned,
    V: serde::de::DeserializeOwned + serde::Serialize,
{
    let items = KeyIter::new(map, deps.storage, 0, map.get_len(deps.storage)?)
        .flatten()
        .collect();
    Ok(items)
}
