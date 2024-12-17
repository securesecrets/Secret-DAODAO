use crate::msg::{
    ExecuteMsg, InfoResponse, InstantiateMsg, MigrateMsg, PendingRewardsResponse, QueryMsg,
    ReceiveMsg,
};
use crate::state::{
    Config, Denom, RewardConfig, CONFIG, LAST_UPDATE_BLOCK, PENDING_REWARDS, REWARD_CONFIG,
    REWARD_PER_TOKEN, USER_REWARD_PER_TOKEN,
};
use crate::ContractError;
use crate::ContractError::{
    InvalidFunds, InvalidSnip20, NoRewardsClaimable, RewardPeriodNotFinished,
};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use shade_protocol::basic_staking::Auth;

use crate::msg::Snip20ReceiveMsg;
use crate::state::Denom::Snip20;
use cosmwasm_std::{
    from_binary, to_binary, Addr, BankMsg, Binary, Coin, CosmosMsg, Deps, DepsMut, Empty, Env,
    MessageInfo, Response, StdError, StdResult, Uint128, Uint256, WasmMsg,
};
use dao_hooks::stake::StakeChangedHookMsg;
use secret_cw2::{get_contract_version, set_contract_version, ContractVersion};
use std::cmp::min;
use std::convert::TryInto;

const CONTRACT_NAME: &str = "crates.io:snip20-stake-external-rewards";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const PREFIX_REVOKED_PERMITS: &str = "revoked_permits";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<Empty>, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    cw_ownable::initialize_owner(deps.storage, deps.api, msg.owner.as_deref())?;

    let reward_token = match msg.reward_token {
        Denom::Native(denom) => Denom::Native(denom),
        Snip20(addr) => Snip20(deps.api.addr_validate(addr.as_ref())?),
    };

    // Verify contract provided is a staking contract
    let _: snip20_stake::msg::TotalStakedAtHeightResponse = deps.querier.query_wasm_smart(
        msg.staking_contract_code_hash.clone(),
        &msg.staking_contract,
        &snip20_stake::msg::QueryMsg::TotalStakedAtHeight { height: None },
    )?;

    let config = Config {
        staking_contract: deps.api.addr_validate(&msg.staking_contract)?,
        reward_token,
        staking_contract_code_hash: msg.staking_contract_code_hash.clone(),
        reward_token_code_hash: msg.reward_token_code_hash.unwrap_or_default(),
    };
    CONFIG.save(deps.storage, &config)?;

    if msg.reward_duration == 0 {
        return Err(ContractError::ZeroRewardDuration {});
    }

    let reward_config = RewardConfig {
        period_finish: 0,
        reward_rate: Uint128::zero(),
        reward_duration: msg.reward_duration,
    };
    REWARD_CONFIG.save(deps.storage, &reward_config)?;

    Ok(Response::new()
        .add_attribute("owner", msg.owner.unwrap_or_else(|| "None".to_string()))
        .add_attribute("staking_contract", config.staking_contract)
        .add_attribute(
            "reward_token",
            match config.reward_token {
                Denom::Native(denom) => denom,
                Snip20(addr) => addr.into_string(),
            },
        )
        .add_attribute("reward_rate", reward_config.reward_rate)
        .add_attribute("period_finish", reward_config.period_finish.to_string())
        .add_attribute("reward_duration", reward_config.reward_duration.to_string()))
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
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<Empty>, ContractError> {
    match msg {
        ExecuteMsg::StakeChangeHook(msg) => execute_stake_changed(deps, env, info, msg),
        ExecuteMsg::Claim { auth } => execute_claim(deps, env, info, auth),
        ExecuteMsg::Fund { auth } => execute_fund_native(deps, env, info, auth),
        ExecuteMsg::Receive(msg) => execute_receive(deps, env, info, msg),
        ExecuteMsg::UpdateRewardDuration { new_duration } => {
            execute_update_reward_duration(deps, env, info, new_duration)
        }
        ExecuteMsg::UpdateOwnership(action) => execute_update_owner(deps, info, env, action),
    }
}

pub fn execute_receive(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    wrapper: Snip20ReceiveMsg,
) -> Result<Response<Empty>, ContractError> {
    let msg: ReceiveMsg = from_binary(&wrapper.msg.unwrap())?;
    let config = CONFIG.load(deps.storage)?;
    let sender = deps.api.addr_validate(wrapper.sender.as_ref())?;
    if config.reward_token != Denom::Snip20(info.sender) {
        return Err(InvalidSnip20 {});
    };
    match msg {
        ReceiveMsg::Fund { auth } => execute_fund(deps, env, auth, sender, wrapper.amount),
    }
}

pub fn execute_fund_native(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    auth: Auth,
) -> Result<Response<Empty>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    match config.reward_token {
        Denom::Native(denom) => {
            let amount = secret_utils::must_pay(&info, &denom).map_err(|_| InvalidFunds {})?;
            execute_fund(deps, env, auth, info.sender, amount)
        }
        Snip20(_) => Err(InvalidFunds {}),
    }
}

pub fn execute_fund(
    mut deps: DepsMut,
    env: Env,
    auth: Auth,
    sender: Addr,
    amount: Uint128,
) -> Result<Response<Empty>, ContractError> {
    cw_ownable::assert_owner(deps.storage, &sender)?;

    update_rewards(&mut deps, &env, auth, &sender)?;
    let reward_config = REWARD_CONFIG.load(deps.storage)?;
    if reward_config.period_finish > env.block.height {
        return Err(RewardPeriodNotFinished {});
    }
    let new_reward_config = RewardConfig {
        period_finish: env.block.height + reward_config.reward_duration,
        reward_rate: amount
            .checked_div(Uint128::from(reward_config.reward_duration))
            .map_err(StdError::divide_by_zero)?,
        // As we're not changing the value and changing the value
        // validates that the duration is non-zero we don't need to
        // check here.
        reward_duration: reward_config.reward_duration,
    };

    if new_reward_config.reward_rate == Uint128::zero() {
        return Err(ContractError::RewardRateLessThenOnePerBlock {});
    };

    REWARD_CONFIG.save(deps.storage, &new_reward_config)?;
    LAST_UPDATE_BLOCK.save(deps.storage, &env.block.height)?;

    Ok(Response::new()
        .add_attribute("action", "fund")
        .add_attribute("amount", amount)
        .add_attribute("new_reward_rate", new_reward_config.reward_rate.to_string()))
}

pub fn execute_stake_changed(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: StakeChangedHookMsg,
) -> Result<Response<Empty>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.staking_contract {
        return Err(ContractError::InvalidHookSender {});
    };
    match msg {
        StakeChangedHookMsg::Stake { addr, auth, .. } => execute_stake(deps, env, auth, addr),
        StakeChangedHookMsg::Unstake { addr, auth, .. } => execute_unstake(deps, env, auth, addr),
    }
}

pub fn execute_stake(
    mut deps: DepsMut,
    env: Env,
    auth: Auth,
    addr: Addr,
) -> Result<Response<Empty>, ContractError> {
    update_rewards(&mut deps, &env, auth, &addr)?;
    Ok(Response::new().add_attribute("action", "stake"))
}

pub fn execute_unstake(
    mut deps: DepsMut,
    env: Env,
    auth: Auth,
    addr: Addr,
) -> Result<Response<Empty>, ContractError> {
    update_rewards(&mut deps, &env, auth, &addr)?;
    Ok(Response::new().add_attribute("action", "unstake"))
}

pub fn execute_claim(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    auth: Auth,
) -> Result<Response<Empty>, ContractError> {
    update_rewards(&mut deps, &env, auth, &info.sender)?;
    let rewards = PENDING_REWARDS
        .load(deps.storage, info.sender.clone())
        .map_err(|_| NoRewardsClaimable {})?;
    if rewards == Uint128::zero() {
        return Err(ContractError::NoRewardsClaimable {});
    }
    PENDING_REWARDS.save(deps.storage, info.sender.clone(), &Uint128::zero())?;
    let config = CONFIG.load(deps.storage)?;
    let transfer_msg = get_transfer_msg(
        info.sender,
        rewards,
        config.reward_token,
        config.reward_token_code_hash,
    )?;
    Ok(Response::new()
        .add_message(transfer_msg)
        .add_attribute("action", "claim")
        .add_attribute("amount", rewards))
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

pub fn get_transfer_msg(
    recipient: Addr,
    amount: Uint128,
    denom: Denom,
    code_hash: String,
) -> StdResult<CosmosMsg> {
    match denom {
        Denom::Native(denom) => Ok(BankMsg::Send {
            to_address: recipient.into_string(),
            amount: vec![Coin { denom, amount }],
        }
        .into()),
        Denom::Snip20(addr) => {
            let snip20_msg = to_binary(&secret_toolkit::snip20::HandleMsg::Transfer {
                recipient: recipient.into_string(),
                amount,
                memo: None,
                padding: None,
            })?;
            Ok(WasmMsg::Execute {
                contract_addr: addr.into_string(),
                msg: snip20_msg,
                funds: vec![],
                code_hash,
            }
            .into())
        }
    }
}

pub fn update_rewards(deps: &mut DepsMut, env: &Env, auth: Auth, addr: &Addr) -> StdResult<()> {
    let config = CONFIG.load(deps.storage)?;
    let reward_per_token = get_reward_per_token(
        deps.as_ref(),
        env,
        &config.staking_contract,
        config.staking_contract_code_hash,
    )?;
    REWARD_PER_TOKEN.save(deps.storage, &reward_per_token)?;

    let earned_rewards = get_rewards_earned(
        deps.as_ref(),
        reward_per_token,
        &config.staking_contract,
        auth.clone(),
        addr,
    )?;

    PENDING_REWARDS.update::<_, StdError>(deps.storage, addr.clone(), |r| {
        Ok(r.unwrap_or_default() + earned_rewards)
    })?;

    USER_REWARD_PER_TOKEN.save(deps.storage, addr.clone(), &reward_per_token)?;
    let last_time_reward_applicable = get_last_time_reward_applicable(deps.as_ref(), env)?;
    LAST_UPDATE_BLOCK.save(deps.storage, &last_time_reward_applicable)?;
    Ok(())
}

pub fn get_reward_per_token(
    deps: Deps,
    env: &Env,
    staking_contract: &Addr,
    staking_contract_code_hash: String,
) -> StdResult<Uint256> {
    let reward_config = REWARD_CONFIG.load(deps.storage)?;
    let total_staked = get_total_staked(deps, staking_contract, staking_contract_code_hash)?;
    let last_time_reward_applicable = get_last_time_reward_applicable(deps, env)?;
    let last_update_block = LAST_UPDATE_BLOCK.load(deps.storage).unwrap_or_default();
    let prev_reward_per_token = REWARD_PER_TOKEN.load(deps.storage).unwrap_or_default();
    let additional_reward_per_token = if total_staked == Uint128::zero() {
        Uint256::zero()
    } else {
        // It is impossible for this to overflow as total rewards can never exceed max value of
        // Uint128 as total tokens in existence cannot exceed Uint128
        let numerator = reward_config
            .reward_rate
            .full_mul(Uint128::from(
                last_time_reward_applicable - last_update_block,
            ))
            .checked_mul(scale_factor())?;
        let denominator = Uint256::from(total_staked);
        numerator.checked_div(denominator)?
    };

    Ok(prev_reward_per_token + additional_reward_per_token)
}

pub fn get_rewards_earned(
    deps: Deps,
    reward_per_token: Uint256,
    staking_contract: &Addr,
    auth: Auth,
    addr: &Addr,
) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    let staked_balance = Uint256::from(get_staked_balance(
        deps,
        staking_contract,
        config.staking_contract_code_hash,
        auth.clone(),
    )?);
    let user_reward_per_token = USER_REWARD_PER_TOKEN
        .load(deps.storage, addr.clone())
        .unwrap_or_default();
    let reward_factor = reward_per_token.checked_sub(user_reward_per_token)?;
    Ok(staked_balance
        .checked_mul(reward_factor)?
        .checked_div(scale_factor())?
        .try_into()?)
}

fn get_last_time_reward_applicable(deps: Deps, env: &Env) -> StdResult<u64> {
    let reward_config = REWARD_CONFIG.load(deps.storage)?;
    Ok(min(env.block.height, reward_config.period_finish))
}

fn get_total_staked(
    deps: Deps,
    contract_addr: &Addr,
    staking_contract_code_hash: String,
) -> StdResult<Uint128> {
    let msg = snip20_stake::msg::QueryMsg::TotalStakedAtHeight { height: None };
    let resp: snip20_stake::msg::TotalStakedAtHeightResponse =
        deps.querier
            .query_wasm_smart(staking_contract_code_hash, contract_addr, &msg)?;
    Ok(resp.total)
}

fn get_staked_balance(
    deps: Deps,
    contract_address: &Addr,
    staking_contract_code_hash: String,
    auth: Auth,
) -> StdResult<Uint128> {
    let msg = snip20_stake::msg::QueryMsg::StakedBalanceAtHeight { auth, height: None };
    let resp: snip20_stake::msg::StakedBalanceAtHeightResponse = deps.querier.query_wasm_smart(
        staking_contract_code_hash,
        contract_address.to_string(),
        &msg,
    )?;
    Ok(resp.balance)
}

pub fn execute_update_reward_duration(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    new_duration: u64,
) -> Result<Response<Empty>, ContractError> {
    cw_ownable::assert_owner(deps.storage, &info.sender)?;

    let mut reward_config = REWARD_CONFIG.load(deps.storage)?;
    if reward_config.period_finish > env.block.height {
        return Err(ContractError::RewardPeriodNotFinished {});
    };

    if new_duration == 0 {
        return Err(ContractError::ZeroRewardDuration {});
    }

    let old_duration = reward_config.reward_duration;
    reward_config.reward_duration = new_duration;
    REWARD_CONFIG.save(deps.storage, &reward_config)?;

    Ok(Response::new()
        .add_attribute("action", "update_reward_duration")
        .add_attribute("new_duration", new_duration.to_string())
        .add_attribute("old_duration", old_duration.to_string()))
}

fn scale_factor() -> Uint256 {
    Uint256::from(10u8).pow(39)
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Info {} => Ok(to_binary(&query_info(deps, env)?)?),
        QueryMsg::Ownership {} => to_binary(&cw_ownable::get_ownership(deps.storage)?),
        QueryMsg::GetPendingRewards { auth, addr } => {
            Ok(to_binary(&query_pending_rewards(deps, env, *auth, addr)?)?)
        }
    }
}

pub fn query_info(deps: Deps, _env: Env) -> StdResult<InfoResponse> {
    let config = CONFIG.load(deps.storage)?;
    let reward = REWARD_CONFIG.load(deps.storage)?;
    Ok(InfoResponse { config, reward })
}

pub fn query_pending_rewards(
    deps: Deps,
    env: Env,
    auth: Auth,
    addr: Addr,
) -> StdResult<PendingRewardsResponse> {
    let config = CONFIG.load(deps.storage)?;
    let reward_per_token = get_reward_per_token(
        deps,
        &env,
        &config.staking_contract,
        config.staking_contract_code_hash,
    )?;
    let earned_rewards = get_rewards_earned(
        deps,
        reward_per_token,
        &config.staking_contract,
        auth.clone(),
        &addr,
    )?;

    let existing_rewards = PENDING_REWARDS
        .load(deps.storage, addr.clone())
        .unwrap_or_default();
    let pending_rewards = earned_rewards + existing_rewards;
    Ok(PendingRewardsResponse {
        address: addr.to_string(),
        pending_rewards,
        denom: config.reward_token,
        last_update_block: LAST_UPDATE_BLOCK.load(deps.storage).unwrap_or_default(),
    })
}

#[cfg(test)]
#[allow(unused)]
mod tests {
    use std::borrow::BorrowMut;

    use crate::{state::Denom, ContractError};

    use cosmwasm_std::{
        coin, from_binary, testing::mock_info, to_binary, Addr, ContractInfo, Empty, MessageInfo,
        Uint128,
    };
    use cw_ownable::{Action, Ownership, OwnershipError};
    use secret_utils::Duration;
    use shade_protocol::basic_staking::Auth;
    use snip20_base::msg::{ExecuteMsg as Snip20ExecuteMsg, InitialBalance, QueryAnswer};

    use secret_multi_test::{
        next_block, App, BankSudo, Contract, ContractWrapper, Executor, SudoMsg,
    };

    use crate::msg::{ExecuteMsg, InfoResponse, PendingRewardsResponse, QueryMsg, ReceiveMsg};

    const OWNER: &str = "owner";
    const ADDR1: &str = "addr0001";
    const ADDR2: &str = "addr0002";
    const ADDR3: &str = "addr0003";

    pub fn contract_rewards() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            crate::contract::execute,
            crate::contract::instantiate,
            crate::contract::query,
        )
        .with_migrate(crate::contract::migrate);
        Box::new(contract)
    }

    pub fn contract_staking() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            snip20_stake::contract::execute,
            snip20_stake::contract::instantiate,
            snip20_stake::contract::query,
        );
        Box::new(contract)
    }

    pub fn contract_snip20() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            snip20_base::contract::execute,
            snip20_base::contract::instantiate,
            snip20_base::contract::query,
        );
        Box::new(contract)
    }

    fn contract_query_auth() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            query_auth::contract::execute,
            query_auth::contract::instantiate,
            query_auth::contract::query,
        );
        Box::new(contract)
    }

    fn mock_app() -> App {
        App::default()
    }

    fn instantiate_snip20(app: &mut App, initial_balances: Vec<InitialBalance>) -> ContractInfo {
        let snip20_info = app.store_code(contract_snip20());
        let msg = snip20_base::msg::InstantiateMsg {
            name: String::from("Test"),
            symbol: String::from("TEST"),
            decimals: 6,
            initial_balances: Some(initial_balances),
            admin: None,
            prng_seed: to_binary("seed").unwrap(),
            config: None,
            supported_denoms: None,
        };

        app.instantiate_contract(
            snip20_info,
            Addr::unchecked(ADDR1),
            &msg,
            &[],
            "snip20",
            None,
        )
        .unwrap()
    }

    fn instantiate_staking(
        app: &mut App,
        snip20_info: ContractInfo,
        unstaking_duration: Option<Duration>,
        query_auth: shade_protocol::Contract,
    ) -> ContractInfo {
        let staking_info = app.store_code(contract_staking());
        let msg = snip20_stake::msg::InstantiateMsg {
            owner: Some(OWNER.to_string()),
            token_address: snip20_info.address.to_string(),
            unstaking_duration,
            token_code_hash: Some(snip20_info.code_hash),
            query_auth: query_auth.into(),
        };
        app.instantiate_contract(
            staking_info,
            Addr::unchecked(ADDR1),
            &msg,
            &[],
            "staking",
            Some("admin".to_string()),
        )
        .unwrap()
    }

    fn instantiate_query_auth(app: &mut App) -> ContractInfo {
        let query_auth_info = app.store_code(contract_query_auth());
        let msg = shade_protocol::contract_interfaces::query_auth::InstantiateMsg {
            admin_auth: shade_protocol::Contract {
                address: Addr::unchecked("admin_contract"),
                code_hash: "code_hash".to_string(),
            },
            prng_seed: to_binary("seed").unwrap(),
        };

        app.instantiate_contract(
            query_auth_info,
            Addr::unchecked(ADDR1),
            &msg,
            &[],
            "query_auth",
            None,
        )
        .unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn stake_tokens<T: Into<String>>(
        app: &mut App,
        staking_addr: &Addr,
        staking_code_hash: String,
        snip20_addr: &Addr,
        snip20_code_hash: String,
        sender: T,
        amount: u128,
        auth: Box<Auth>,
    ) {
        let msg = Snip20ExecuteMsg::Send {
            recipient: staking_addr.to_string(),
            recipient_code_hash: Some(staking_code_hash),
            amount: Uint128::new(amount),
            msg: Some(to_binary(&snip20_stake::msg::ReceiveMsg::Stake { auth }).unwrap()),
            memo: None,
            decoys: None,
            entropy: None,
            padding: None,
        };
        app.execute_contract(
            Addr::unchecked(sender),
            &ContractInfo {
                address: snip20_addr.clone(),
                code_hash: snip20_code_hash,
            },
            &msg,
            &[],
        )
        .unwrap();
    }

    fn create_viewing_key(app: &mut App, contract_info: ContractInfo, info: MessageInfo) -> String {
        let msg = shade_protocol::contract_interfaces::query_auth::ExecuteMsg::CreateViewingKey {
            entropy: "entropy".to_string(),
            padding: None,
        };
        let res = app
            .execute_contract(info.sender, &contract_info, &msg, &[])
            .unwrap();
        let mut viewing_key = String::new();
        let data: shade_protocol::contract_interfaces::query_auth::ExecuteAnswer =
            from_binary(&res.data.unwrap()).unwrap();
        if let shade_protocol::contract_interfaces::query_auth::ExecuteAnswer::CreateViewingKey {
            key,
        } = data
        {
            viewing_key = key;
        };
        viewing_key
    }

    fn create_viewing_key_snip20(
        app: &mut App,
        contract_info: ContractInfo,
        info: MessageInfo,
    ) -> String {
        let msg = snip20_base::msg::ExecuteMsg::CreateViewingKey {
            entropy: "entropy".to_string(),
            padding: None,
        };
        let res = app
            .execute_contract(info.sender, &contract_info, &msg, &[])
            .unwrap();
        let mut viewing_key = String::new();
        let data: snip20_base::msg::ExecuteAnswer =
            from_binary(&res.data.unwrap()).unwrap();
        if let snip20_base::msg::ExecuteAnswer::CreateViewingKey { key } = data {
            viewing_key = key;
        };
        viewing_key
    }

    fn unstake_tokens(
        app: &mut App,
        staking_info: &ContractInfo,
        address: &str,
        amount: u128,
        auth: Auth,
    ) {
        let msg = snip20_stake::msg::ExecuteMsg::Unstake {
            auth,
            amount: Uint128::new(amount),
        };
        app.execute_contract(Addr::unchecked(address), staking_info, &msg, &[])
            .unwrap();
    }

    fn setup_staking_contract(
        app: &mut App,
        initial_balances: Vec<InitialBalance>,
    ) -> (ContractInfo, ContractInfo, ContractInfo) {
        // Instantiate snip20 contract
        let snip20_info = instantiate_snip20(app, initial_balances.clone());
        app.update_block(next_block);
        // Instantiate query_auth contract
        let query_auth_info = instantiate_query_auth(app);
        app.update_block(next_block);
        // Instantiate staking contract
        let staking_info = instantiate_staking(
            app,
            snip20_info.clone(),
            None,
            shade_protocol::Contract {
                address: query_auth_info.clone().address,
                code_hash: query_auth_info.clone().code_hash,
            },
        );
        app.update_block(next_block);
        for coin in initial_balances {
            let info = mock_info(&coin.address, &[]);
            let viewing_key = create_viewing_key(app, query_auth_info.clone(), info.clone());
            stake_tokens(
                app,
                &staking_info.clone().address,
                staking_info.clone().code_hash,
                &snip20_info.clone().address,
                snip20_info.clone().code_hash,
                coin.address.clone(),
                coin.amount.u128(),
                Box::new(Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: coin.address,
                }),
            );
        }
        (staking_info, snip20_info, query_auth_info)
    }

    fn setup_reward_contract(
        app: &mut App,
        staking_info: ContractInfo,
        reward_token: Denom,
        reward_token_code_hash: Option<String>,
        owner: Addr,
    ) -> ContractInfo {
        let reward_info = app.store_code(contract_rewards());
        let msg = crate::msg::InstantiateMsg {
            owner: Some(owner.clone().into_string()),
            staking_contract: staking_info.address.clone().into_string(),
            staking_contract_code_hash: staking_info.clone().code_hash,
            reward_token,
            reward_token_code_hash,
            reward_duration: 100000,
        };
        let reward_contract_info = app
            .instantiate_contract(reward_info, owner, &msg, &[], "reward", None)
            .unwrap();
        let msg = snip20_stake::msg::ExecuteMsg::AddHook {
            addr: reward_contract_info.clone().address.to_string(),
            code_hash: reward_contract_info.clone().code_hash,
        };
        let _result = app
            .execute_contract(Addr::unchecked(OWNER), &staking_info, &msg, &[])
            .unwrap();
        reward_contract_info
    }

    fn get_balance_snip20(
        app: &App,
        snip20_info: ContractInfo,
        address: String,
        key: String,
    ) -> Uint128 {
        let msg = snip20_base::msg::QueryMsg::Balance { address, key };
        let result: snip20_base::msg::QueryAnswer = app
            .wrap()
            .query_wasm_smart(snip20_info.code_hash, snip20_info.address.to_string(), &msg)
            .unwrap();
        let mut balance = Uint128::zero();
        if let QueryAnswer::Balance { amount } = result {
            balance = amount;
        }
        balance
    }

    fn get_balance_native<T: Into<String>, U: Into<String>>(
        app: &App,
        address: T,
        denom: U,
    ) -> Uint128 {
        app.wrap().query_balance(address, denom).unwrap().amount
    }

    fn get_ownership<T: Into<String>, C: Into<String>>(
        app: &App,
        address: T,
        code_hash: C,
    ) -> Ownership<Addr> {
        app.wrap()
            .query_wasm_smart(code_hash, address, &QueryMsg::Ownership {})
            .unwrap()
    }

    fn assert_pending_rewards(
        app: &mut App,
        reward_contract_info: ContractInfo,
        auth: Auth,
        addr: Addr,
        expected: u128,
    ) {
        let res: PendingRewardsResponse = app
            .borrow_mut()
            .wrap()
            .query_wasm_smart(
                reward_contract_info.code_hash,
                reward_contract_info.address.to_string(),
                &QueryMsg::GetPendingRewards {
                    auth: Box::new(auth),
                    addr,
                },
            )
            .unwrap();
        assert_eq!(res.pending_rewards, Uint128::new(expected));
    }

    fn claim_rewards(app: &mut App, reward_contract_info: ContractInfo, auth: Auth) {
        let msg = ExecuteMsg::Claim { auth: auth.clone() };
        let mut sender = String::new();
        if let Auth::ViewingKey { address, .. } = auth {
            sender = address
        }
        app.borrow_mut()
            .execute_contract(Addr::unchecked(sender), &reward_contract_info, &msg, &[])
            .unwrap();
    }

    fn fund_rewards_snip20(
        app: &mut App,
        admin: &Addr,
        snip20_info: ContractInfo,
        reward_contract_info: ContractInfo,
        amount: u128,
        auth: Auth,
    ) {
        let fund_sub_msg = to_binary(&ReceiveMsg::Fund { auth }).unwrap();
        let fund_msg = Snip20ExecuteMsg::Send {
            recipient: reward_contract_info.address.clone().into_string(),
            recipient_code_hash: Some(reward_contract_info.clone().code_hash),
            amount: Uint128::new(amount),
            msg: Some(fund_sub_msg),
            memo: None,
            decoys: None,
            entropy: None,
            padding: None,
        };
        let _res = app
            .borrow_mut()
            .execute_contract(admin.clone(), &snip20_info, &fund_msg, &[])
            .unwrap();
    }

    #[test]
    fn test_zero_rewards_duration() {
        let mut app = mock_app();
        let admin = Addr::unchecked(OWNER);
        app.borrow_mut().update_block(|b| b.height = 0);
        let denom = "utest".to_string();
        let (staking_info, _, _) = setup_staking_contract(&mut app, vec![]);
        let reward_funding = vec![coin(100000000, denom.clone())];
        app.sudo(SudoMsg::Bank({
            BankSudo::Mint {
                to_address: admin.to_string(),
                amount: reward_funding,
            }
        }))
        .unwrap();

        let reward_token = Denom::Native(denom);
        let owner = admin;
        let reward_info = app.store_code(contract_rewards());
        let msg = crate::msg::InstantiateMsg {
            owner: Some(owner.clone().into_string()),
            staking_contract: staking_info.address.to_string(),
            staking_contract_code_hash: staking_info.code_hash,
            reward_token,
            reward_token_code_hash: None,
            reward_duration: 0,
        };
        let err: ContractError = app
            .instantiate_contract(reward_info, owner, &msg, &[], "reward", None)
            .unwrap_err()
            .downcast()
            .unwrap();
        assert_eq!(err, ContractError::ZeroRewardDuration {})
    }

    #[test]
    fn update_rewards() {
        let mut app = mock_app();
        let admin = Addr::unchecked(OWNER);
        app.borrow_mut().update_block(|b| b.height = 0);
        let initial_balances = vec![
            InitialBalance {
                address: ADDR1.to_string(),
                amount: Uint128::new(100),
            },
            InitialBalance {
                address: ADDR2.to_string(),
                amount: Uint128::new(50),
            },
            InitialBalance {
                address: ADDR3.to_string(),
                amount: Uint128::new(50),
            },
        ];
        let denom = "utest".to_string();
        let (staking_info, _snip20_info, query_auth_info) =
            setup_staking_contract(&mut app, initial_balances);

        let info = mock_info(ADDR1, &[]);
        let viewing_key_addr1 = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

        let info = mock_info(OWNER, &[]);
        let viewing_key_admin = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

        let reward_funding = vec![coin(200000000, denom.clone())];
        app.sudo(SudoMsg::Bank({
            BankSudo::Mint {
                to_address: admin.to_string(),
                amount: reward_funding.clone(),
            }
        }))
        .unwrap();
        // Add funding to Addr1 to make sure it can't update staking contract
        app.sudo(SudoMsg::Bank({
            BankSudo::Mint {
                to_address: ADDR1.to_string(),
                amount: reward_funding.clone(),
            }
        }))
        .unwrap();
        let reward_contract_info = setup_reward_contract(
            &mut app,
            staking_info,
            Denom::Native(denom.clone()),
            None,
            admin.clone(),
        );

        app.borrow_mut().update_block(|b| b.height = 1000);

        let fund_msg_addr1 = ExecuteMsg::Fund {
            auth: Auth::ViewingKey {
                key: viewing_key_addr1.clone(),
                address: ADDR1.to_string(),
            },
        };
        let fund_msg_admin = ExecuteMsg::Fund {
            auth: Auth::ViewingKey {
                key: viewing_key_admin.clone(),
                address: OWNER.to_string(),
            },
        };

        // None admin cannot update rewards
        let err: ContractError = app
            .borrow_mut()
            .execute_contract(
                Addr::unchecked(ADDR1),
                &reward_contract_info.clone(),
                &fund_msg_addr1,
                &reward_funding,
            )
            .unwrap_err()
            .downcast()
            .unwrap();

        assert_eq!(err, ContractError::Ownable(OwnershipError::NotOwner));

        let _res = app
            .borrow_mut()
            .execute_contract(
                admin.clone(),
                &reward_contract_info.clone(),
                &fund_msg_admin,
                &reward_funding,
            )
            .unwrap();

        let res: InfoResponse = app
            .borrow_mut()
            .wrap()
            .query_wasm_smart(
                reward_contract_info.clone().code_hash,
                reward_contract_info.clone().address.to_string(),
                &QueryMsg::Info {},
            )
            .unwrap();

        assert_eq!(res.reward.reward_rate, Uint128::new(2000));
        assert_eq!(res.reward.period_finish, 101000);
        assert_eq!(res.reward.reward_duration, 100000);

        // Create new period after old period
        app.borrow_mut().update_block(|b| b.height = 101000);

        let reward_funding = vec![coin(100000000, denom.clone())];
        app.sudo(SudoMsg::Bank({
            BankSudo::Mint {
                to_address: admin.to_string(),
                amount: reward_funding.clone(),
            }
        }))
        .unwrap();
        let _res = app
            .borrow_mut()
            .execute_contract(
                admin.clone(),
                &reward_contract_info.clone(),
                &fund_msg_admin,
                &reward_funding,
            )
            .unwrap();

        let res: InfoResponse = app
            .borrow_mut()
            .wrap()
            .query_wasm_smart(
                reward_contract_info.clone().code_hash,
                reward_contract_info.clone().address.to_string(),
                &QueryMsg::Info {},
            )
            .unwrap();

        assert_eq!(res.reward.reward_rate, Uint128::new(1000));
        assert_eq!(res.reward.period_finish, 201000);
        assert_eq!(res.reward.reward_duration, 100000);

        // Add funds in middle of period returns an error
        app.borrow_mut().update_block(|b| b.height = 151000);

        let reward_funding = vec![coin(200000000, denom)];
        app.sudo(SudoMsg::Bank({
            BankSudo::Mint {
                to_address: admin.to_string(),
                amount: reward_funding.clone(),
            }
        }))
        .unwrap();
        let err = app
            .borrow_mut()
            .execute_contract(
                admin,
                &reward_contract_info.clone(),
                &fund_msg_admin,
                &reward_funding,
            )
            .unwrap_err();
        assert_eq!(
            ContractError::RewardPeriodNotFinished {},
            err.downcast().unwrap()
        );

        let res: InfoResponse = app
            .borrow_mut()
            .wrap()
            .query_wasm_smart(
                reward_contract_info.clone().code_hash,
                reward_contract_info.clone().address.to_string(),
                &QueryMsg::Info {},
            )
            .unwrap();

        assert_eq!(res.reward.reward_rate, Uint128::new(1000));
        assert_eq!(res.reward.period_finish, 201000);
        assert_eq!(res.reward.reward_duration, 100000);
    }

    #[test]
    fn update_reward_duration() {
        let mut app = mock_app();
        let admin = Addr::unchecked(OWNER);
        app.borrow_mut().update_block(|b| b.height = 0);
        let initial_balances = vec![
            InitialBalance {
                address: ADDR1.to_string(),
                amount: Uint128::new(100),
            },
            InitialBalance {
                address: ADDR2.to_string(),
                amount: Uint128::new(50),
            },
            InitialBalance {
                address: ADDR3.to_string(),
                amount: Uint128::new(50),
            },
        ];
        let denom = "utest".to_string();
        let (staking_info, _snip20_info, query_auth_info) =
            setup_staking_contract(&mut app, initial_balances);

        let info = mock_info(OWNER, &[]);
        let viewing_key_admin = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

        let reward_contract_info = setup_reward_contract(
            &mut app,
            staking_info,
            Denom::Native(denom.clone()),
            None,
            admin.clone(),
        );

        let res: InfoResponse = app
            .borrow_mut()
            .wrap()
            .query_wasm_smart(
                reward_contract_info.clone().code_hash,
                reward_contract_info.clone().address.to_string(),
                &QueryMsg::Info {},
            )
            .unwrap();

        assert_eq!(res.reward.reward_rate, Uint128::new(0));
        assert_eq!(res.reward.period_finish, 0);
        assert_eq!(res.reward.reward_duration, 100000);

        // Zero rewards durations are not allowed.
        let msg = ExecuteMsg::UpdateRewardDuration { new_duration: 0 };
        let err: ContractError = app
            .borrow_mut()
            .execute_contract(admin.clone(), &reward_contract_info.clone(), &msg, &[])
            .unwrap_err()
            .downcast()
            .unwrap();
        assert_eq!(err, ContractError::ZeroRewardDuration {});

        let msg = ExecuteMsg::UpdateRewardDuration { new_duration: 10 };
        let _resp = app
            .borrow_mut()
            .execute_contract(admin.clone(), &reward_contract_info.clone(), &msg, &[])
            .unwrap();

        let res: InfoResponse = app
            .borrow_mut()
            .wrap()
            .query_wasm_smart(
                reward_contract_info.clone().code_hash,
                reward_contract_info.clone().address.to_string(),
                &QueryMsg::Info {},
            )
            .unwrap();

        assert_eq!(res.reward.reward_rate, Uint128::new(0));
        assert_eq!(res.reward.period_finish, 0);
        assert_eq!(res.reward.reward_duration, 10);

        // Non-admin cannot update rewards
        let msg = ExecuteMsg::UpdateRewardDuration { new_duration: 100 };
        let err: ContractError = app
            .borrow_mut()
            .execute_contract(
                Addr::unchecked("non-admin"),
                &reward_contract_info.clone(),
                &msg,
                &[],
            )
            .unwrap_err()
            .downcast()
            .unwrap();
        assert_eq!(err, ContractError::Ownable(OwnershipError::NotOwner));

        let reward_funding = vec![coin(1000, denom)];
        app.sudo(SudoMsg::Bank({
            BankSudo::Mint {
                to_address: admin.to_string(),
                amount: reward_funding.clone(),
            }
        }))
        .unwrap();
        // Add funding to Addr1 to make sure it can't update staking contract
        app.sudo(SudoMsg::Bank({
            BankSudo::Mint {
                to_address: ADDR1.to_string(),
                amount: reward_funding.clone(),
            }
        }))
        .unwrap();

        app.borrow_mut().update_block(|b| b.height = 1000);

        let fund_msg = ExecuteMsg::Fund {
            auth: Auth::ViewingKey {
                key: viewing_key_admin.clone(),
                address: OWNER.to_string(),
            },
        };

        let _res = app
            .borrow_mut()
            .execute_contract(
                admin.clone(),
                &reward_contract_info.clone(),
                &fund_msg,
                &reward_funding,
            )
            .unwrap();

        let res: InfoResponse = app
            .borrow_mut()
            .wrap()
            .query_wasm_smart(
                reward_contract_info.clone().code_hash,
                reward_contract_info.clone().address.to_string(),
                &QueryMsg::Info {},
            )
            .unwrap();

        assert_eq!(res.reward.reward_rate, Uint128::new(100));
        assert_eq!(res.reward.period_finish, 1010);
        assert_eq!(res.reward.reward_duration, 10);

        // Cannot update reward period before it finishes
        let msg = ExecuteMsg::UpdateRewardDuration { new_duration: 10 };
        let err: ContractError = app
            .borrow_mut()
            .execute_contract(admin.clone(), &reward_contract_info.clone(), &msg, &[])
            .unwrap_err()
            .downcast()
            .unwrap();
        assert_eq!(err, ContractError::RewardPeriodNotFinished {});

        // Update reward period once rewards are finished
        app.borrow_mut().update_block(|b| b.height = 1010);

        let msg = ExecuteMsg::UpdateRewardDuration { new_duration: 100 };
        let _resp = app
            .borrow_mut()
            .execute_contract(admin, &reward_contract_info.clone(), &msg, &[])
            .unwrap();

        let res: InfoResponse = app
            .borrow_mut()
            .wrap()
            .query_wasm_smart(
                reward_contract_info.clone().code_hash,
                reward_contract_info.clone().address.to_string(),
                &QueryMsg::Info {},
            )
            .unwrap();

        assert_eq!(res.reward.reward_rate, Uint128::new(100));
        assert_eq!(res.reward.period_finish, 1010);
        assert_eq!(res.reward.reward_duration, 100);
    }

    #[test]
    fn test_update_owner() {
        let mut app = mock_app();
        let addr_owner = Addr::unchecked(OWNER);
        app.borrow_mut().update_block(|b| b.height = 0);
        let initial_balances = vec![
            InitialBalance {
                address: ADDR1.to_string(),
                amount: Uint128::new(100),
            },
            InitialBalance {
                address: ADDR2.to_string(),
                amount: Uint128::new(50),
            },
            InitialBalance {
                address: ADDR3.to_string(),
                amount: Uint128::new(50),
            },
        ];
        let denom = "utest".to_string();
        let (staking_info, _snip20_info, _query_auth_info) =
            setup_staking_contract(&mut app, initial_balances);

        let reward_contract_info = setup_reward_contract(
            &mut app,
            staking_info,
            Denom::Native(denom),
            None,
            addr_owner.clone(),
        );

        let owner = get_ownership(
            &app,
            reward_contract_info.clone().address.to_string(),
            reward_contract_info.clone().code_hash,
        )
        .owner;
        assert_eq!(owner, Some(addr_owner.clone()));

        // random addr cannot update owner
        let msg = ExecuteMsg::UpdateOwnership(Action::TransferOwnership {
            new_owner: ADDR1.to_string(),
            expiry: None,
        });
        let err: ContractError = app
            .borrow_mut()
            .execute_contract(
                Addr::unchecked(ADDR1),
                &reward_contract_info.clone(),
                &msg,
                &[],
            )
            .unwrap_err()
            .downcast()
            .unwrap();
        assert_eq!(err, ContractError::Ownable(OwnershipError::NotOwner));

        // owner nominates a new onwer.
        app.borrow_mut()
            .execute_contract(addr_owner.clone(), &reward_contract_info.clone(), &msg, &[])
            .unwrap();

        let ownership = get_ownership(
            &app,
            reward_contract_info.clone().address.to_string(),
            reward_contract_info.clone().code_hash,
        );
        assert_eq!(
            ownership,
            Ownership::<Addr> {
                owner: Some(addr_owner),
                pending_owner: Some(Addr::unchecked(ADDR1)),
                pending_expiry: None,
            }
        );

        // new owner accepts the nomination.
        app.execute_contract(
            Addr::unchecked(ADDR1),
            &reward_contract_info.clone(),
            &ExecuteMsg::UpdateOwnership(Action::AcceptOwnership),
            &[],
        )
        .unwrap();

        let ownership = get_ownership(
            &app,
            reward_contract_info.clone().address.to_string(),
            reward_contract_info.clone().code_hash,
        );
        assert_eq!(
            ownership,
            Ownership::<Addr> {
                owner: Some(Addr::unchecked(ADDR1)),
                pending_owner: None,
                pending_expiry: None,
            }
        );

        // new owner renounces ownership.
        app.execute_contract(
            Addr::unchecked(ADDR1),
            &reward_contract_info.clone(),
            &ExecuteMsg::UpdateOwnership(Action::RenounceOwnership),
            &[],
        )
        .unwrap();

        let ownership = get_ownership(
            &app,
            reward_contract_info.clone().address.to_string(),
            reward_contract_info.clone().code_hash,
        );
        assert_eq!(
            ownership,
            Ownership::<Addr> {
                owner: None,
                pending_owner: None,
                pending_expiry: None,
            }
        );
    }

    #[test]
    fn test_cannot_fund_with_wrong_coin_native() {
        let mut app = mock_app();
        let owner = Addr::unchecked(OWNER);
        app.borrow_mut().update_block(|b| b.height = 0);
        let initial_balances = vec![
            InitialBalance {
                address: ADDR1.to_string(),
                amount: Uint128::new(100),
            },
            InitialBalance {
                address: ADDR2.to_string(),
                amount: Uint128::new(50),
            },
            InitialBalance {
                address: ADDR3.to_string(),
                amount: Uint128::new(50),
            },
        ];
        let denom = "utest".to_string();
        let (staking_info, _snip20_info, query_auth_info) =
            setup_staking_contract(&mut app, initial_balances);

        let info = mock_info(OWNER, &[]);
        let viewing_key_admin = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

        let reward_contract_info = setup_reward_contract(
            &mut app,
            staking_info,
            Denom::Native(denom.clone()),
            None,
            owner.clone(),
        );

        app.borrow_mut().update_block(|b| b.height = 1000);

        // No funding
        let fund_msg = ExecuteMsg::Fund {
            auth: Auth::ViewingKey {
                key: viewing_key_admin.clone(),
                address: OWNER.to_string(),
            },
        };

        let err: ContractError = app
            .borrow_mut()
            .execute_contract(owner.clone(), &reward_contract_info.clone(), &fund_msg, &[])
            .unwrap_err()
            .downcast()
            .unwrap();
        assert_eq!(err, ContractError::InvalidFunds {});

        // Invalid funding
        let invalid_funding = vec![coin(100, "invalid")];
        app.sudo(SudoMsg::Bank({
            BankSudo::Mint {
                to_address: owner.to_string(),
                amount: invalid_funding.clone(),
            }
        }))
        .unwrap();

        let fund_msg = ExecuteMsg::Fund {
            auth: Auth::ViewingKey {
                key: viewing_key_admin.clone(),
                address: OWNER.to_string(),
            },
        };

        let err: ContractError = app
            .borrow_mut()
            .execute_contract(
                owner.clone(),
                &reward_contract_info.clone(),
                &fund_msg,
                &invalid_funding,
            )
            .unwrap_err()
            .downcast()
            .unwrap();
        assert_eq!(err, ContractError::InvalidFunds {});

        // Extra funding
        let extra_funding = vec![coin(100, denom), coin(100, "extra")];
        app.sudo(SudoMsg::Bank({
            BankSudo::Mint {
                to_address: owner.to_string(),
                amount: extra_funding.clone(),
            }
        }))
        .unwrap();

        let fund_msg = ExecuteMsg::Fund {
            auth: Auth::ViewingKey {
                key: viewing_key_admin.clone(),
                address: OWNER.to_string(),
            },
        };

        let err: ContractError = app
            .borrow_mut()
            .execute_contract(
                owner.clone(),
                &reward_contract_info.clone(),
                &fund_msg,
                &extra_funding,
            )
            .unwrap_err()
            .downcast()
            .unwrap();
        assert_eq!(err, ContractError::InvalidFunds {});

        // Snip20 funding fails
        let snip20_info = instantiate_snip20(
            &mut app,
            vec![InitialBalance {
                address: OWNER.to_string(),
                amount: Uint128::new(500000000),
            }],
        );
        let fund_sub_msg = to_binary(&ReceiveMsg::Fund {
            auth: Auth::ViewingKey {
                key: viewing_key_admin.clone(),
                address: OWNER.to_string(),
            },
        })
        .unwrap();
        let fund_msg = snip20_base::msg::ExecuteMsg::Send {
            recipient: reward_contract_info.clone().address.into_string(),
            recipient_code_hash: Some(reward_contract_info.clone().code_hash),
            amount: Uint128::new(100),
            msg: Some(fund_sub_msg),
            memo: None,
            decoys: None,
            entropy: None,
            padding: None,
        };
        let err: ContractError = app
            .borrow_mut()
            .execute_contract(owner, &snip20_info, &fund_msg, &[])
            .unwrap_err()
            .downcast()
            .unwrap();
        assert_eq!(err, ContractError::InvalidSnip20 {});
    }

    #[test]
    fn test_cannot_fund_with_wrong_coin_cw20() {
        let mut app = mock_app();
        let admin = Addr::unchecked(OWNER);
        app.borrow_mut().update_block(|b| b.height = 0);
        let initial_balances = vec![
            InitialBalance {
                address: ADDR1.to_string(),
                amount: Uint128::new(100),
            },
            InitialBalance {
                address: ADDR2.to_string(),
                amount: Uint128::new(50),
            },
            InitialBalance {
                address: ADDR3.to_string(),
                amount: Uint128::new(50),
            },
        ];
        let _denom = "utest".to_string();
        let (staking_info, _snip20_info, query_auth_info) =
            setup_staking_contract(&mut app, initial_balances);

        let info = mock_info(OWNER, &[]);
        let viewing_key_admin = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

        let snip20_info = instantiate_snip20(
            &mut app,
            vec![InitialBalance {
                address: OWNER.to_string(),
                amount: Uint128::new(500000000),
            }],
        );
        let reward_contract_info = setup_reward_contract(
            &mut app,
            staking_info,
            Denom::Snip20(Addr::unchecked("dummy_cw20")),
            Some("Dummy_Snip20_Code_hash".to_string()),
            admin.clone(),
        );

        app.borrow_mut().update_block(|b| b.height = 1000);

        // Test with invalid token
        let fund_sub_msg = to_binary(&ReceiveMsg::Fund {
            auth: Auth::ViewingKey {
                key: viewing_key_admin.clone(),
                address: OWNER.to_string(),
            },
        })
        .unwrap();
        let fund_msg = snip20_base::msg::ExecuteMsg::Send {
            recipient: reward_contract_info.clone().address.into_string(),
            recipient_code_hash: Some(reward_contract_info.clone().code_hash),
            amount: Uint128::new(100),
            msg: Some(fund_sub_msg),
            memo: None,
            decoys: None,
            entropy: None,
            padding: None,
        };
        let err: ContractError = app
            .borrow_mut()
            .execute_contract(admin.clone(), &snip20_info, &fund_msg, &[])
            .unwrap_err()
            .downcast()
            .unwrap();
        assert_eq!(err, ContractError::InvalidSnip20 {});

        // Test does not work when funded with native
        let invalid_funding = vec![coin(100, "invalid")];
        app.sudo(SudoMsg::Bank({
            BankSudo::Mint {
                to_address: admin.to_string(),
                amount: invalid_funding.clone(),
            }
        }))
        .unwrap();

        let fund_msg = ExecuteMsg::Fund {
            auth: Auth::ViewingKey {
                key: viewing_key_admin.clone(),
                address: OWNER.to_string(),
            },
        };

        let err: ContractError = app
            .borrow_mut()
            .execute_contract(admin, &reward_contract_info, &fund_msg, &invalid_funding)
            .unwrap_err()
            .downcast()
            .unwrap();
        assert_eq!(err, ContractError::InvalidFunds {})
    }

    #[test]
    fn test_small_rewards() {
        // This test was added due to a bug in the contract not properly paying out small reward
        // amounts due to floor division
        let mut app = mock_app();
        let admin = Addr::unchecked(OWNER);
        app.borrow_mut().update_block(|b| b.height = 0);
        let initial_balances = vec![
            InitialBalance {
                address: ADDR1.to_string(),
                amount: Uint128::new(100),
            },
            InitialBalance {
                address: ADDR2.to_string(),
                amount: Uint128::new(50),
            },
            InitialBalance {
                address: ADDR3.to_string(),
                amount: Uint128::new(50),
            },
        ];
        let denom = "utest".to_string();
        let (staking_info, _snip20_info, query_auth_info) =
            setup_staking_contract(&mut app, initial_balances);

        let info = mock_info(OWNER, &[]);
        let viewing_key_admin = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

        let info = mock_info(ADDR1, &[]);
        let viewing_key_addr1 = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

        let info = mock_info(ADDR2, &[]);
        let viewing_key_addr2 = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

        let info = mock_info(ADDR3, &[]);
        let viewing_key_addr3 = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

        let reward_funding = vec![coin(1000000, denom.clone())];
        app.sudo(SudoMsg::Bank({
            BankSudo::Mint {
                to_address: admin.to_string(),
                amount: reward_funding.clone(),
            }
        }))
        .unwrap();
        let reward_contract_info = setup_reward_contract(
            &mut app,
            staking_info,
            Denom::Native(denom),
            None,
            admin.clone(),
        );

        app.borrow_mut().update_block(|b| b.height = 1000);

        let fund_msg = ExecuteMsg::Fund {
            auth: Auth::ViewingKey {
                key: viewing_key_admin.clone(),
                address: OWNER.to_string(),
            },
        };

        let _res = app
            .borrow_mut()
            .execute_contract(
                admin,
                &reward_contract_info.clone(),
                &fund_msg,
                &reward_funding,
            )
            .unwrap();

        let res: InfoResponse = app
            .borrow_mut()
            .wrap()
            .query_wasm_smart(
                reward_contract_info.clone().code_hash,
                reward_contract_info.clone().address.to_string(),
                &QueryMsg::Info {},
            )
            .unwrap();

        assert_eq!(res.reward.reward_rate, Uint128::new(10));
        assert_eq!(res.reward.period_finish, 101000);
        assert_eq!(res.reward.reward_duration, 100000);

        app.borrow_mut().update_block(next_block);
        assert_pending_rewards(
            &mut app,
            reward_contract_info.clone(),
            Auth::ViewingKey {
                key: viewing_key_addr1,
                address: ADDR1.to_string(),
            },
            Addr::unchecked(ADDR1),
            5,
        );
        assert_pending_rewards(
            &mut app,
            reward_contract_info.clone(),
            Auth::ViewingKey {
                key: viewing_key_addr2,
                address: ADDR2.to_string(),
            },
            Addr::unchecked(ADDR2),
            2,
        );
        assert_pending_rewards(
            &mut app,
            reward_contract_info.clone(),
            Auth::ViewingKey {
                key: viewing_key_addr3,
                address: ADDR3.to_string(),
            },
            Addr::unchecked(ADDR3),
            2,
        );
    }

    #[test]
    fn test_zero_reward_rate_failed() {
        // This test is due to a bug when funder provides rewards config that results in less then 1
        // reward per block which rounds down to zer0
        let mut app = mock_app();
        let admin = Addr::unchecked(OWNER);
        app.borrow_mut().update_block(|b| b.height = 0);
        let initial_balances = vec![
            InitialBalance {
                address: ADDR1.to_string(),
                amount: Uint128::new(100),
            },
            InitialBalance {
                address: ADDR2.to_string(),
                amount: Uint128::new(50),
            },
            InitialBalance {
                address: ADDR3.to_string(),
                amount: Uint128::new(50),
            },
        ];
        let denom = "utest".to_string();
        let (staking_info, _snip20_info, query_auth_info) =
            setup_staking_contract(&mut app, initial_balances);

        let info = mock_info(OWNER, &[]);
        let viewing_key_admin = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

        let reward_funding = vec![coin(10000, denom.clone())];
        app.sudo(SudoMsg::Bank({
            BankSudo::Mint {
                to_address: admin.to_string(),
                amount: reward_funding.clone(),
            }
        }))
        .unwrap();
        let reward_contract_info = setup_reward_contract(
            &mut app,
            staking_info,
            Denom::Native(denom),
            None,
            admin.clone(),
        );

        app.borrow_mut().update_block(|b| b.height = 1000);

        let fund_msg = ExecuteMsg::Fund {
            auth: Auth::ViewingKey {
                key: viewing_key_admin.clone(),
                address: OWNER.to_string(),
            },
        };

        let _res = app
            .borrow_mut()
            .execute_contract(admin, &reward_contract_info, &fund_msg, &reward_funding)
            .unwrap_err();
    }
}
