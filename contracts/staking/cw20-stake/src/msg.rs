use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Binary, Uint128};
// use snip20_reference_impl::receiver::Snip20ReceiveMsg;

use secret_utils::Duration;

use cw_ownable::cw_ownable_execute;

pub use secret_cw_controllers::ClaimsResponse;
// so that consumers don't need a cw_ownable dependency to consume
// this contract's queries.
pub use cw_ownable::Ownership;

#[cw_serde]
pub struct InstantiateMsg {
    // Owner can update all configs including changing the owner. This will generally be a DAO.
    pub owner: Option<String>,
    pub token_address: String,
    pub unstaking_duration: Option<Duration>,
}
#[cw_serde]
#[serde(rename_all = "snake_case")]
pub struct Snip20ReceiveMsg {
    pub sender: String,
    pub from: String,
    pub amount: Uint128,
    pub memo: Option<String>,
    pub msg: Binary,
}

#[cw_ownable_execute]
#[cw_serde]
pub enum ExecuteMsg {
    Receive(Snip20ReceiveMsg),
    Unstake { amount: Uint128 },
    Claim {},
    UpdateConfig { duration: Option<Duration> },
    AddHook { addr: String },
    RemoveHook { addr: String },
    CreateViewingKey { entropy: String },
    SetViewingKey { key: String },
}

#[cw_serde]
pub enum ExecuteAnswer {
    // Native
    Receive { status: ResponseStatus },
    Unstake { status: ResponseStatus },

    // Base
    Claim { status: ResponseStatus },
    UpdateConfig { status: ResponseStatus },
    AddHook { status: ResponseStatus },
    RemoveHook { status: ResponseStatus },
    CreateViewingKey { key: String },
    SetViewingKey { status: ResponseStatus },
}

#[cw_serde]
pub enum ReceiveMsg {
    Stake {},
    Fund {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(StakedBalanceAtHeightResponse)]
    StakedBalanceAtHeight {
        key: String,
        address: String,
        height: Option<u64>,
    },
    #[returns(TotalStakedAtHeightResponse)]
    TotalStakedAtHeight {
        key: String,
        address: String,
        height: Option<u64>,
    },
    #[returns(StakedValueResponse)]
    StakedValue { key: String, address: String },
    #[returns(TotalValueResponse)]
    TotalValue { key: String, address: String },
    #[returns(crate::state::Config)]
    GetConfig { key: String, address: String },
    #[returns(ClaimsResponse)]
    Claims { key: String, address: String },
    #[returns(GetHooksResponse)]
    GetHooks { key: String, address: String },
    #[returns(ListStakersResponse)]
    ListStakers {
        key: String,
        address: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    #[returns(::cw_ownable::Ownership::<::cosmwasm_std::Addr>)]
    Ownership {},
}

#[cw_serde]
pub enum MigrateMsg {
    /// Migrates the contract from version one to version two. This
    /// will remove the contract's current manager, and require a
    /// nomination -> acceptance flow for future ownership transfers.
    FromV1 {},
}

#[cw_serde]
pub struct StakedBalanceAtHeightResponse {
    pub balance: Uint128,
    pub height: u64,
}

#[cw_serde]
pub struct TotalStakedAtHeightResponse {
    pub total: Uint128,
    pub height: u64,
}

#[cw_serde]
pub struct StakedValueResponse {
    pub value: Uint128,
}

#[cw_serde]
pub struct TotalValueResponse {
    pub total: Uint128,
}

#[cw_serde]
pub struct GetHooksResponse {
    pub hooks: Vec<String>,
}

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
pub enum ResponseStatus {
    Success,
    Failure,
}
