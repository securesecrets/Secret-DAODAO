use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use cw_hooks::HookItem;
use dao_dao_macros::{active_query, native_token_query, voting_module_query};
use dao_voting::threshold::ActiveThreshold;
use secret_utils::Duration;
use shade_protocol::{basic_staking::Auth, utils::asset::RawContract};

#[cw_serde]
pub enum TokenInfo {
    /// Uses an existing Token Factory token and creates a new issuer contract.
    /// Full setup, such as transferring ownership or setting up MsgSetBeforeSendHook,
    /// must be done manually.
    Existing {
        /// Token factory denom
        denom: String,
    },
    // NOTE* There is right now no way to create new token so will be using existing token

    // Creates a new Token Factory token via the issue contract with the DAO automatically
    // setup as admin and owner.

    // New(NewTokenInfo),

    // Uses a factory contract that must return the denom, optionally a Token Contract address.
    // The binary must serialize to a `WasmMsg::Execute` message.
    // Validation happens in the factory contract itself, so be sure to use a
    // trusted factory contract.
    // Factory(Binary),
}

#[cw_serde]
pub struct InstantiateMsg {
    /// New or existing native token to use for voting power.
    pub token_info: TokenInfo,
    /// How long until the tokens become liquid again
    pub unstaking_duration: Option<Duration>,
    /// The number or percentage of tokens that must be staked
    /// for the DAO to be active
    pub active_threshold: Option<ActiveThreshold>,
    pub dao_code_hash: String,
    pub query_auth: RawContract,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Stakes tokens with the contract to get voting power in the DAO
    Stake {},
    /// Unstakes tokens so that they begin unbonding
    Unstake { amount: Uint128 },
    /// Updates the contract configuration
    UpdateConfig { duration: Option<Duration> },
    /// Claims unstaked tokens that have completed the unbonding period
    Claim {},
    /// Sets the active threshold to a new value. Only the
    /// instantiator of this contract (a DAO most likely) may call this
    /// method.
    UpdateActiveThreshold {
        new_threshold: Option<ActiveThreshold>,
    },
    /// Adds a hook that fires on staking / unstaking
    AddHook { addr: String, code_hash: String },
    /// Removes a hook that fires on staking / unstaking
    RemoveHook { addr: String, code_hash: String },
}

#[native_token_query]
#[active_query]
#[voting_module_query]
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(crate::state::Config)]
    GetConfig {},
    #[returns(secret_cw_controllers::ClaimsResponse)]
    Claims { auth: Auth },
    #[returns(ListStakersResponse)]
    ListStakers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    #[returns(dao_voting::threshold::ActiveThresholdResponse)]
    ActiveThreshold {},
    #[returns(GetHooksResponse)]
    GetHooks {},
    #[returns(Option<cosmwasm_std::Addr>)]
    TokenContract {},
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub struct ListStakersResponse {
    pub stakers: Vec<StakerBalanceResponse>,
}

#[cw_serde]
pub struct StakerBalanceResponse {
    pub address: String,
    pub balance: Uint128,
}

#[cw_serde]
pub struct GetHooksResponse {
    pub hooks: Vec<HookItem>,
}
