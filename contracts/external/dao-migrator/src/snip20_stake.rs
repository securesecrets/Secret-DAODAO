use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum MigrateMsg {
    /// Migrates the contract from version one to version two. This
    /// will remove the contract's current manager, and require a
    /// nomination -> acceptance flow for future ownership transfers.
    FromV1 {},
}
