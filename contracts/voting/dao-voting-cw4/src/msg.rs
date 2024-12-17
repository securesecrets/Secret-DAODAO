use cosmwasm_schema::{cw_serde, QueryResponses};
use dao_dao_macros::voting_module_query;
use shade_protocol::utils::asset::RawContract;

#[cw_serde]
pub enum GroupContract {
    Existing {
        address: String,
    },
    New {
        cw4_group_code_id: u64,
        cw4_group_code_hash: String,
        initial_members: Vec<cw4::Member>,
        query_auth: Option<RawContract>,
    },
}

#[cw_serde]
pub struct InstantiateMsg {
    pub group_contract: GroupContract,
}

#[cw_serde]
pub enum ExecuteMsg {}

#[allow(clippy::large_enum_variant)]
#[voting_module_query]
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(dao_interface::state::AnyContractInfo)]
    GroupContract {},
}

#[cw_serde]
pub struct MigrateMsg {}
