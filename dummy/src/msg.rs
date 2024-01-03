use schemars::JsonSchema;
use secret_utils::Duration;
use serde::{Deserialize, Serialize};
use cw_ownable::cw_ownable_execute;
use cosmwasm_std::{Uint128,Binary};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    // Owner can update all configs including changing the owner. This will generally be a DAO.
    pub owner: Option<String>,
    pub token_address: String,
    pub unstaking_duration: Option<Duration>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Snip20ReceiveMsg {
    pub sender: String,
    pub from: String,
    pub amount: Uint128,
    pub memo: Option<String>,
    pub msg: Binary,
}



#[cw_ownable_execute]
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    // Receive(Snip20ReceiveMsg),
    Unstake { amount: Uint128 },
    // Claim {},
    // UpdateConfig { duration: Option<Duration> },
    // AddHook { addr: String },
    // RemoveHook { addr: String },
    // CreateViewingKey { entropy: String },
    // SetViewingKey { key: String },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum ExecuteAnswer {
    // Native
    Receive { status: ResponseStatus },
    Unstake { status: ResponseStatus },

    // // Base
    // Claim { status: ResponseStatus },
    // UpdateConfig { status: ResponseStatus },
    // AddHook { status: ResponseStatus },
    // RemoveHook { status: ResponseStatus },
    // CreateViewingKey { key: String },
    // SetViewingKey { status: ResponseStatus },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum ReceiveMsg {
    Stake {},
    Fund {},
}


#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // GetCount returns the current count as a json-encoded number
    GetCount {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub struct CountResponse {
    pub count: i32,
}


#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum ResponseStatus {
    Success,
    Failure,
}
