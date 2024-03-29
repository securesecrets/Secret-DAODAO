use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use cosmwasm_std::{Addr, Uint128};
use secret_cw2::ContractVersion;
use secret_utils::Expiration;

use crate::state::{Config, ProposalModule,VotingModuleInfo};

/// Relevant state for the governance module. Returned by the
/// `DumpState` query.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct DumpStateResponse {
    /// Optional DAO Admin
    pub admin: Addr,
    /// The governance contract's config.
    pub config: Config,
    // True if the contract is currently paused.
    pub pause_info: PauseInfoResponse,
    /// The governance contract's version.
    pub version: ContractVersion,
    /// The governance modules associated with the governance
    /// contract.
    pub proposal_modules: Vec<ProposalModule>,
    /// The voting module associated with the governance contract.
    pub voting_module: VotingModuleInfo,
    /// The number of active proposal modules.
    pub active_proposal_module_count: u32,
    /// The total number of proposal modules.
    pub total_proposal_module_count: u32,
}

/// Information about if the contract is currently paused.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum PauseInfoResponse {
    Paused { expiration: Expiration },
    Unpaused {},
}


/// Returned by the `GetItem` query.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct GetItemResponse {
    /// `None` if no item with the provided key was found, `Some`
    /// otherwise.
    pub item: Option<String>,
}

/// Returned by the `Cw20Balances` query.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Snip20BalanceResponse {
    /// The address of the token.
    pub addr: String,
    /// The contract's balance.
    pub balance: Uint128,
}

/// Returned by the `AdminNomination` query.
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct AdminNominationResponse {
    /// The currently nominated admin or None if no nomination is
    /// pending.
    pub nomination: Option<Addr>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct SubDao {
    /// The contract address of the SubDAO
    pub addr: String,
    pub code_hash: String,
    /// The purpose/constitution for the SubDAO
    pub charter: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct DaoURIResponse {
    pub dao_uri: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]pub struct ProposalModuleCountResponse {

    /// The number of active proposal modules.
    pub active_proposal_module_count: u32,
    /// The total number of proposal modules.
    pub total_proposal_module_count: u32,
}
