use schemars::JsonSchema;
use secret_toolkit::utils::InitCallback;
use secret_utils::Duration;
use serde::{Deserialize, Serialize};
use shade_protocol::utils::asset::RawContract;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    // Owner can update all configs including changing the owner. This will generally be a DAO.
    pub owner: Option<String>,
    pub token_address: String,
    pub token_code_hash: Option<String>,
    pub unstaking_duration: Option<Duration>,
    pub query_auth: RawContract,
}

impl InitCallback for InstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}
