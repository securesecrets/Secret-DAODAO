//! types used for migrating modules of the DAO with migrating core
//! copyo of the types from dao-migrator contract.

use crate::query::SubDao;
use crate::state::ModuleInstantiateInfo;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct MigrateParams {
    pub migrator_code_id: u64,
    pub params: MigrateV1ToV2,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct MigrateV1ToV2 {
    pub sub_daos: Vec<SubDao>,
    pub migration_params: MigrationModuleParams,
    pub v1_code_ids_and_hashes: V1CodeIdsAndHashes,
    pub v2_code_ids_and_hashes: V2CodeIdsAndHashes,
}

// code ids for the v1 contracts
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct V1CodeIdsAndHashes {
    pub proposal_single: u64,
    pub proposal_single_code_hash: String,
    pub cw4_voting: u64,
    pub cw4_voting_code_hash: String,
    pub snip20_stake: u64,
    pub snip20_stake_code_hash: String,
    pub snip20_staked_balances_voting: u64,
    pub snip20_staked_balances_code_hash: String,
}

// code ids for the new contracts
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct V2CodeIdsAndHashes {
    pub proposal_single: u64,
    pub proposal_single_code_hash: String,
    pub cw4_voting: u64,
    pub cw4_voting_code_hash: String,
    pub snip20_stake: u64,
    pub snip20_stake_code_hash: String,
    pub snip20_staked_balances_voting: u64,
    pub snip20_staked_balances_code_hash: String,
}

/// The params we need to provide for migration msgs
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct ProposalParams {
    pub close_proposal_on_execution_failure: bool,
    pub pre_propose_info: PreProposeInfo,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct MigrationModuleParams {
    // General
    /// Rather or not to migrate the stake_cw20 contract and its
    /// manager. If this is not set to true and a stake_cw20
    /// contract is detected in the DAO's configuration the
    /// migration will be aborted.
    pub migrate_stake_snip20_manager: Option<bool>,
    // dao_proposal_single
    pub proposal_params: Vec<(String, ProposalParams)>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum PreProposeInfo {
    /// Anyone may create a proposal free of charge.
    AnyoneMayPropose {},
    /// The module specified in INFO has exclusive rights to proposal
    /// creation.
    ModuleMayPropose { info: ModuleInstantiateInfo },
}
