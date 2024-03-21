use cosmwasm_schema::QueryResponses;
use cosmwasm_std::{Addr, Binary, Uint128};
use cw_hooks::HookItem;
use cw_ownable::cw_ownable_execute;
use schemars::JsonSchema;
use secret_utils::Duration;
use serde::{Deserialize, Serialize};

pub use secret_cw_controllers::ClaimsResponse;
// so that consumers don't need a cw_ownable dependency to consume
// this contract's queries.
pub use cw_ownable::Ownership;
use shade_protocol::{basic_staking::Auth, utils::asset::RawContract};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    // Owner can update all configs including changing the owner. This will generally be a DAO.
    pub owner: Option<String>,
    pub token_address: String,
    pub token_code_hash: Option<String>,
    pub unstaking_duration: Option<Duration>,
    pub query_auth: RawContract,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Snip20ReceiveMsg {
    pub sender: Addr,
    pub from: Addr,
    pub amount: Uint128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    pub msg: Option<Binary>,
}

#[cw_ownable_execute]
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Receive(Snip20ReceiveMsg),
    Unstake { amount: Uint128 },
    Claim {},
    UpdateConfig { duration: Option<Duration> },
    AddHook { addr: String, code_hash: String },
    RemoveHook { addr: String, code_hash: String },
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct CreateViewingKeyResponse {
    pub key: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum ExecuteAnswer {
    // Native
    Receive { status: ResponseStatus },
    Unstake { status: ResponseStatus },

    // Base
    Claim { status: ResponseStatus },
    UpdateConfig { status: ResponseStatus },
    AddHook { status: ResponseStatus },
    RemoveHook { status: ResponseStatus },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum ReceiveMsg {
    Stake {},
    Fund {},
}

#[derive(Serialize, Deserialize, Clone, JsonSchema, Debug, QueryResponses)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    #[returns(StakedBalanceAtHeightResponse)]
    StakedBalanceAtHeight { auth: Auth, height: Option<u64> },
    #[returns(TotalStakedAtHeightResponse)]
    TotalStakedAtHeight { height: Option<u64> },
    #[returns(StakedValueResponse)]
    StakedValue { auth: Auth },
    #[returns(TotalValueResponse)]
    TotalValue {},
    #[returns(crate::state::Config)]
    GetConfig {},
    #[returns(ClaimsResponse)]
    Claims { auth: Auth },
    #[returns(GetHooksResponse)]
    GetHooks {},
    #[returns(ListStakersResponse)]
    ListStakers {},
    #[returns(::cw_ownable::Ownership::<::cosmwasm_std::Addr>)]
    Ownership {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum MigrateMsg {
    /// Migrates the contract from version one to version two. This
    /// will remove the contract's current manager, and require a
    /// nomination -> acceptance flow for future ownership transfers.
    FromV1 {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct StakedBalanceAtHeightResponse {
    pub balance: Uint128,
    pub height: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct TotalStakedAtHeightResponse {
    pub total: Uint128,
    pub height: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct StakedValueResponse {
    pub value: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct TotalValueResponse {
    pub total: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct GetHooksResponse {
    pub hooks: Vec<HookItem>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ListStakersResponse {
    pub stakers: Vec<StakerBalanceResponse>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct StakerBalanceResponse {
    pub address: String,
    pub balance: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum ResponseStatus {
    Success,
    Failure,
}
