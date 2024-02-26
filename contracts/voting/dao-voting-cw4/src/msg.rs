use cosmwasm_schema::{cw_serde, QueryResponses};
use dao_dao_macros::voting_module_query;
use dao_interface::state::AnyContractInfo;

#[cw_serde]
pub enum GroupContract {
    Existing {
        address: String,
        code_hash: String,
    },
    New {
        cw4_group_code_id: u64,
        cw4_group_code_hash: String,
        initial_members: Vec<cw4::Member>,
    },
}

#[cw_serde]
pub struct InstantiateMsg {
    pub group_contract: GroupContract,
    pub dao_code_hash: String,
}

#[cw_serde]
pub enum ExecuteMsg {}

#[voting_module_query]
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(AnyContractInfo)]
    GroupContract {},
}

#[cw_serde]
pub struct MigrateMsg {}
