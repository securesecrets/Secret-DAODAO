use schemars::JsonSchema;
use secret_toolkit::utils::{HandleCallback, InitCallback};
use secret_utils::Duration;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    // Owner can update all configs including changing the owner. This will generally be a DAO.
    pub owner: Option<String>,
    pub token_address: String,
    pub token_code_hash: Option<String>,
    pub unstaking_duration: Option<Duration>,
}

impl InitCallback for InstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    CreateViewingKey { entropy: String },
}

impl HandleCallback for ExecuteMsg {
    const BLOCK_SIZE: usize = 256;
}
