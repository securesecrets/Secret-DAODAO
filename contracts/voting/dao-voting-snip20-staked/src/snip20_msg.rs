#![allow(clippy::field_reassign_with_default)] // This is triggered in `#[derive(JsonSchema)]`

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cosmwasm_std::{ Binary, Uint128};
use secret_toolkit::permit::Permit;

#[cfg_attr(test, derive(Eq, PartialEq))]
#[derive(Serialize, Deserialize, Clone, JsonSchema)]
pub struct InitialBalance {
    pub address: String,
    pub amount: Uint128,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InstantiateMsg {
    pub name: String,
    pub admin: Option<String>,
    pub symbol: String,
    pub decimals: u8,
    pub initial_balances: Option<Vec<InitialBalance>>,
    pub prng_seed: Binary,
    pub config: Option<InitConfig>,
    pub supported_denoms: Option<Vec<String>>,
}

/// This type represents optional configuration values which can be overridden.
/// All values are optional and have defaults which are more private by default,
/// but can be overridden if necessary
#[derive(Serialize, Deserialize, JsonSchema, Clone, Default, Debug)]
pub struct InitConfig {
    /// Indicates whether the total supply is public or should be kept secret.
    /// default: False
    public_total_supply: Option<bool>,
    /// Indicates whether deposit functionality should be enabled
    /// default: False
    enable_deposit: Option<bool>,
    /// Indicates whether redeem functionality should be enabled
    /// default: False
    enable_redeem: Option<bool>,
    /// Indicates whether mint functionality should be enabled
    /// default: False
    enable_mint: Option<bool>,
    /// Indicates whether burn functionality should be enabled
    /// default: False
    enable_burn: Option<bool>,
    /// Indicated whether an admin can modify supported denoms
    /// default: False
    can_modify_denoms: Option<bool>,
}

// #[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
// #[cfg_attr(test, derive(Eq, PartialEq))]
// #[serde(rename_all = "snake_case")]
// pub enum QueryMsg {
//     TokenInfo {},
//     TokenConfig {},
//     ContractStatus {},
//     ExchangeRate {},
//     Allowance {
//         owner: String,
//         spender: String,
//         key: String,
//     },
//     AllowancesGiven {
//         owner: String,
//         key: String,
//         page: Option<u32>,
//         page_size: u32,
//     },
//     AllowancesReceived {
//         spender: String,
//         key: String,
//         page: Option<u32>,
//         page_size: u32,
//     },
//     Balance {
//         address: String,
//         key: String,
//     },
//     TransferHistory {
//         address: String,
//         key: String,
//         page: Option<u32>,
//         page_size: u32,
//         should_filter_decoys: bool,
//     },
//     TransactionHistory {
//         address: String,
//         key: String,
//         page: Option<u32>,
//         page_size: u32,
//         should_filter_decoys: bool,
//     },
//     Minters {},
//     WithPermit {
//         permit: Permit,
//         query: QueryWithPermit,
//     },
// }

// #[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
// #[cfg_attr(test, derive(Eq, PartialEq))]
// #[serde(rename_all = "snake_case")]
// pub enum QueryWithPermit {
//     Allowance {
//         owner: String,
//         spender: String,
//     },
//     AllowancesGiven {
//         owner: String,
//         page: Option<u32>,
//         page_size: u32,
//     },
//     AllowancesReceived {
//         spender: String,
//         page: Option<u32>,
//         page_size: u32,
//     },
//     Balance {},
//     TransferHistory {
//         page: Option<u32>,
//         page_size: u32,
//         should_filter_decoys: bool,
//     },
//     TransactionHistory {
//         page: Option<u32>,
//         page_size: u32,
//         should_filter_decoys: bool,
//     },
// }


// #[derive(Serialize, Deserialize, JsonSchema, Debug)]
// #[serde(rename_all = "snake_case")]
// pub enum QueryAnswer {
//     TokenInfo {
//         name: String,
//         symbol: String,
//         decimals: u8,
//         total_supply: Option<Uint128>,
//     },
//     TokenConfig {
//         public_total_supply: bool,
//         deposit_enabled: bool,
//         redeem_enabled: bool,
//         mint_enabled: bool,
//         burn_enabled: bool,
//         supported_denoms: Vec<String>,
//     },
//     ContractStatus {
//         status: ContractStatusLevel,
//     },
//     ExchangeRate {
//         rate: Uint128,
//         denom: String,
//     },
//     Allowance {
//         spender: Addr,
//         owner: Addr,
//         allowance: Uint128,
//         expiration: Option<u64>,
//     },
//     AllowancesGiven {
//         owner: Addr,
//         allowances: Vec<AllowanceGivenResult>,
//         count: u32,
//     },
//     AllowancesReceived {
//         spender: Addr,
//         allowances: Vec<AllowanceReceivedResult>,
//         count: u32,
//     },
//     Balance {
//         amount: Uint128,
//     },
//     TransferHistory {
//         txs: Vec<Tx>,
//         total: Option<u64>,
//     },
//     TransactionHistory {
//         txs: Vec<ExtendedTx>,
//         total: Option<u64>,
//     },
//     ViewingKeyError {
//         msg: String,
//     },
//     Minters {
//         minters: Vec<Addr>,
//     },
// }

// #[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
// pub struct AllowanceGivenResult {
//     pub spender: Addr,
//     pub allowance: Uint128,
//     pub expiration: Option<u64>,
// }

// #[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
// pub struct AllowanceReceivedResult {
//     pub owner: Addr,
//     pub allowance: Uint128,
//     pub expiration: Option<u64>,
// }

// #[derive(Serialize, Deserialize, Clone, JsonSchema, Debug)]
// #[cfg_attr(test, derive(Eq, PartialEq))]
// #[serde(rename_all = "snake_case")]
// pub enum ResponseStatus {
//     Success,
//     Failure,
// }

// #[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
// #[serde(rename_all = "snake_case")]
// pub enum ContractStatusLevel {
//     NormalRun,
//     StopAllButRedeems,
//     StopAll,
// }

// pub fn status_level_to_u8(status_level: ContractStatusLevel) -> u8 {
//     match status_level {
//         ContractStatusLevel::NormalRun => 0,
//         ContractStatusLevel::StopAllButRedeems => 1,
//         ContractStatusLevel::StopAll => 2,
//     }
// }

// pub fn u8_to_status_level(status_level: u8) -> StdResult<ContractStatusLevel> {
//     match status_level {
//         0 => Ok(ContractStatusLevel::NormalRun),
//         1 => Ok(ContractStatusLevel::StopAllButRedeems),
//         2 => Ok(ContractStatusLevel::StopAll),
//         _ => Err(StdError::generic_err("Invalid state level")),
//     }
// }

