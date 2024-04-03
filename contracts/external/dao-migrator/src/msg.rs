use cosmwasm_schema::{cw_serde, QueryResponses};
use dao_interface::query::SubDao;

use crate::types::{MigrationParams, V1CodeIdsAndHashes, V2CodeIdsAndHashes};

#[cw_serde]
pub struct MigrateV1ToV2 {
    pub sub_daos: Vec<SubDao>,
    pub dao_code_hash: String,
    pub migration_params: MigrationParams,
    pub v1_code_ids_and_hashes: V1CodeIdsAndHashes,
    pub v2_code_ids_and_hashes: V2CodeIdsAndHashes,
}

pub type InstantiateMsg = MigrateV1ToV2;

pub type ExecuteMsg = MigrateV1ToV2;

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {}
