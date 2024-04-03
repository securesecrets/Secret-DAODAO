use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary, Uint128};
use schemars::JsonSchema;
use secret_utils::Duration;
use serde::{Deserialize, Serialize};
use shade_protocol::{basic_staking::Auth, utils::asset::RawContract};

use crate::state::VotingContractInfo;

#[cw_serde]
pub struct InstantiateMsg {
    // To determine voting power
    pub voting_contract: String,
    pub voting_contract_hash: String,

    // period after which the funds can be claimed
    pub funding_period: Duration,
    // snapshot for evaluating the voting power
    pub distribution_height: u64,

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

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct TokenInfo {
    pub address: Addr,
    pub code_hash: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    Receive(Snip20ReceiveMsg),
    FundNative {},
    ClaimSnip20 { auth: Auth, tokens: Vec<TokenInfo> },
    ClaimNatives { auth: Auth, denoms: Vec<String> },
    ClaimAll { auth: Auth },
    SetSnip20sCodeHash { token_info: Vec<TokenInfo> },
}

#[cw_serde]
pub enum QueryMsg {
    TotalPower {},
    VotingContract {},
    NativeDenoms {},
    Snip20Tokens {},
    NativeEntitlement {
        auth: Auth,
        denom: String,
    },
    Snip20Entitlement {
        auth: Auth,
        token: String,
    },
    NativeEntitlements {
        auth: Auth,
        start_at: Option<String>,
        limit: Option<u32>,
    },
    Snip20Entitlements {
        auth: Auth,
        start_at: Option<String>,
        limit: Option<u32>,
    },
}

#[cw_serde]
pub struct VotingContractResponse {
    // voting power contract being used
    pub contract: VotingContractInfo,
    // height at which voting power is being determined
    pub distribution_height: u64,
}

#[cw_serde]
pub struct TotalPowerResponse {
    // total power at the distribution height
    pub total_power: Uint128,
}

#[cw_serde]
pub enum MigrateMsg {
    RedistributeUnclaimedFunds { distribution_height: u64 },
}

#[cw_serde]
pub struct DenomResponse {
    pub contract_balance: Uint128,
    pub denom: String,
}

#[cw_serde]
pub struct Snip20Response {
    pub contract_balance: Uint128,
    pub token: String,
}

#[cw_serde]
pub struct NativeEntitlementResponse {
    pub amount: Uint128,
    pub denom: String,
}

#[cw_serde]
pub struct Snip20EntitlementResponse {
    pub amount: Uint128,
    pub token_contract: Addr,
}
