use cosmwasm_schema::cw_serde;
use dao_snip721_extensions::roles::{ExecuteExt, MetadataExt, QueryExt};
use serde::{Serialize,Deserialize};
use schemars::JsonSchema;
use cosmwasm_std::Addr;

use crate::snip721::{Snip721ExecuteMsg, Snip721InstantiateMsg,Snip721QueryMsg};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InstantiateMsg {
    pub snip721_code_id: u64,

    pub snip721_code_hash: String,

    pub label: String,

    pub snip721_init_msg: Snip721InstantiateMsg,
}

#[cw_serde]
pub struct InstantiateResponse {
    pub contract_address: Addr,
    pub code_hash: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub enum ExecuteMsg {
    Snip721Execute(snip721_reference_impl::msg::ExecuteMsg),
    ExtensionExecute(ExecuteExt),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    Snip721Query(snip721_reference_impl::msg::QueryMsg),
    ExtensionQuery (QueryExt),
}

