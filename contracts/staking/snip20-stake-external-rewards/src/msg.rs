use crate::state::{Config, Denom, RewardConfig};
use cosmwasm_schema::QueryResponses;
use cosmwasm_std::{Addr, Binary, Uint128};
use dao_hooks::stake::StakeChangedHookMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use secret_cw_controllers::ClaimsResponse;
// so that consumers don't need a cw_ownable dependency to consume
// this contract's queries.
pub use cw_ownable::Ownership;

use cw_ownable::cw_ownable_execute;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    pub owner: Option<String>,
    pub staking_contract: String,
    pub staking_contract_code_hash: String,
    pub reward_token: Denom,
    pub reward_token_code_hash: Option<String>,
    pub reward_duration: u64,
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
    StakeChangeHook(StakeChangedHookMsg),
    Claim {},
    Receive(Snip20ReceiveMsg),
    Fund {},
    UpdateRewardDuration { new_duration: u64 },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MigrateMsg {
    /// Migrates from version 0.2.6 to 2.0.0. The significant changes
    /// being the addition of a two-step ownership transfer using
    /// `cw_ownable` and the removal of the manager. Migrating will
    /// automatically remove the current manager.
    FromV1 {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub enum ReceiveMsg {
    Fund {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug, QueryResponses)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    #[returns(InfoResponse)]
    Info {},
    #[returns(PendingRewardsResponse)]
    GetPendingRewards { address: String },
    #[returns(::cw_ownable::Ownership<::cosmwasm_std::Addr>)]
    Ownership {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct InfoResponse {
    pub config: Config,
    pub reward: RewardConfig,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
pub struct PendingRewardsResponse {
    pub address: String,
    pub pending_rewards: Uint128,
    pub denom: Denom,
    pub last_update_block: u64,
}
