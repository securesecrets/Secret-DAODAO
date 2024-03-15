use crate::state::ModuleInstantiateCallback;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct NftFactoryCallback {
    pub nft_contract: String,
    pub nft_code_hash: String,
    pub module_instantiate_callback: Option<ModuleInstantiateCallback>,
}
