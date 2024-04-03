use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state::CheckedCounterparty;

/// Information about the token being used on one side of the escrow.
#[cw_serde]
pub enum TokenInfo {
    /// A native token.
    Native { denom: String, amount: Uint128 },
    /// A cw20 token.
    Snip20 {
        contract_addr: String,
        code_hash: String,
        amount: Uint128,
    },
}

/// Information about a counterparty in this escrow transaction and
/// their promised funds.
#[cw_serde]
pub struct Counterparty {
    /// The address of the counterparty.
    pub address: String,
    /// The funds they have promised to provide.
    pub promise: TokenInfo,
}

#[cw_serde]
pub struct InstantiateMsg {
    pub counterparty_one: Counterparty,
    pub counterparty_two: Counterparty,
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
#[cw_serde]
pub enum ExecuteMsg {
    /// Used to provide cw20 tokens to satisfy a funds promise.
    Receive(Snip20ReceiveMsg),
    /// Provides native tokens to satisfy a funds promise.
    Fund {},
    /// Withdraws provided funds. Only allowed if the other
    /// counterparty has yet to provide their promised funds.
    Withdraw {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    // Gets the current status of the escrow transaction.
    #[returns(crate::msg::StatusResponse)]
    Status {},
}

#[cw_serde]
pub struct StatusResponse {
    pub counterparty_one: CheckedCounterparty,
    pub counterparty_two: CheckedCounterparty,
}

#[cw_serde]
pub struct MigrateMsg {}
