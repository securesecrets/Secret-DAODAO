use cosmwasm_schema:: QueryResponses;
use cosmwasm_std::Uint128;
use secret_cw2::ContractVersion;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
#[derive(QueryResponses)]
pub enum Query {
    /// Returns the token contract address, if set.
    #[returns(::cosmwasm_std::Addr)]
    TokenContract {},
    /// Returns the native token denom, if used.
    #[returns(DenomResponse)]
    Denom {},
    /// Returns the voting power for an address at a given height.
    #[returns(VotingPowerAtHeightResponse)]
    VotingPowerAtHeight {
        address: ::std::string::String,
        height: ::std::option::Option<::std::primitive::u64>,
    },
    /// Returns the total voting power at a given block heigh.
    #[returns(TotalPowerAtHeightResponse)]
    TotalPowerAtHeight {
        height: ::std::option::Option<::std::primitive::u64>,
    },
    /// Returns the address of the DAO this module belongs to.
    #[returns(cosmwasm_std::Addr)]
    Dao {},
    /// Returns contract version info.
    #[returns(InfoResponse)]
    Info {},
    /// Whether the DAO is active or not.
    #[returns(::std::primitive::bool)]
    IsActive {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ActiveThresholdQuery {
    ActiveThreshold {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct VotingPowerAtHeightResponse {
    pub power: Uint128,
    pub height: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct TotalPowerAtHeightResponse {
    pub power: Uint128,
    pub height: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InfoResponse {
    pub info: ContractVersion,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]pub struct IsActiveResponse {

    pub active: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]pub struct DenomResponse {

    pub denom: String,
}
