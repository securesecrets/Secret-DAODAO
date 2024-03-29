use crate::snip20_msg::InitialBalance;
use cosmwasm_schema::QueryResponses;
use cosmwasm_std::Uint128;
use dao_dao_macros::{active_query, cw20_token_query, voting_module_query};
use dao_voting::threshold::ActiveThreshold;
use schemars::JsonSchema;
use secret_utils::Duration;
use serde::{Deserialize, Serialize};

/// Information about the staking contract to be used with this voting
/// module.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum StakingInfo {
    Existing {
        /// Address of an already instantiated staking contract.
        staking_contract_address: String,
        /// code hash of an already instantiated staking contract.
        staking_contract_code_hash: String,
    },
    New {
        /// Code ID for staking contract to instantiate.
        staking_code_id: u64,
        /// Code hash for staking contract to instantiate.
        staking_code_hash: String,
        /// label for the contract
        label: String,
        /// See corresponding field in cw20-stake's
        /// instantiation. This will be used when instantiating the
        /// new staking contract.
        unstaking_duration: Option<Duration>,
    },
}

#[derive(Serialize, Deserialize, Clone, JsonSchema)]
#[allow(clippy::large_enum_variant)]
pub enum Snip20TokenInfo {
    Existing {
        /// Address of an already instantiated cw20 token contract.
        address: String,
        /// Code hash of an already instantiated cw20 token contract.
        code_hash: String,
        /// Information about the staking contract to use.
        staking_contract: StakingInfo,
    },
    New {
        /// Code ID for snip20 token contract.
        code_id: u64,
        /// Code hash for snip20 token contract
        code_hash: String,
        /// Label to use for instantiated snip20 contract.
        label: String,

        name: String,
        symbol: String,
        decimals: u8,
        initial_balances: Vec<InitialBalance>,

        staking_code_id: u64,
        staking_code_hash: String,
        unstaking_duration: Option<Duration>,
        initial_dao_balance: Option<Uint128>,
    },
}

#[derive(Serialize, Deserialize, Clone, JsonSchema)]
pub struct InstantiateMsg {
    pub token_info: Snip20TokenInfo,
    /// The number or percentage of tokens that must be staked
    /// for the DAO to be active
    pub active_threshold: Option<ActiveThreshold>,
    pub dao_code_hash: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum ExecuteMsg {
    /// Sets the active threshold to a new value. Only the
    /// instantiator this contract (a DAO most likely) may call this
    /// method.
    UpdateActiveThreshold {
        new_threshold: Option<ActiveThreshold>,
    },
}

#[voting_module_query]
#[cw20_token_query]
#[active_query]
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug, QueryResponses)]
pub enum QueryMsg {
    /// Gets the address of the cw20-stake contract this voting module
    /// is wrapping.
    #[returns(cosmwasm_std::Addr)]
    StakingContract {},
    #[returns(dao_voting::threshold::ActiveThresholdResponse)]
    ActiveThreshold {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct MigrateMsg {}
