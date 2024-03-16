use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;
use dao_snip721_extensions::roles::{ExecuteExt, QueryExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::snip721::{self, Snip721ExecuteMsg, Snip721QueryMsg};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InstantiateMsg {
    /// Code ID for snip721 token contract.
    pub code_id: u64,
    /// Code hash for snip721 token contract.
    pub code_hash: String,
    /// Label to use for instantiated snip721 contract.
    pub label: String,
    /// NFT collection name
    pub name: String,
    /// NFT collection symbol
    pub symbol: String,

    /// entropy used for prng seed
    pub entropy: String,

    /// optional privacy configuration for the contract
    pub config: Option<snip721::InstantiateConfig>,
}

#[cw_serde]
pub struct InstantiateResponse {
    pub contract_address: Addr,
    pub code_hash: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub enum ExecuteMsg {
    Snip721Execute(Box<Snip721ExecuteMsg>),
    ExtensionExecute(ExecuteExt),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, QueryResponses)]
pub enum QueryMsg {
    #[returns(())]
    Snip721Query(Snip721QueryMsg),
    #[returns(())]
    ExtensionQuery(QueryExt),
    #[returns(crate::state::Config)]
    GetNftContractInfo {},
}
