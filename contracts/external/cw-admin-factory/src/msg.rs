use cosmwasm_schema::{cw_serde, QueryResponses};
use dao_interface::state::ModuleInstantiateInfo;

#[cw_serde]
pub struct InstantiateMsg {}

#[cw_serde]
pub enum ExecuteMsg {
    /// Instantiates the target contract with the provided instantiate message and code id and
    /// updates the contract's admin to be itself.
    InstantiateContractWithSelfAdmin { module_info: ModuleInstantiateInfo },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {}

#[cw_serde]
pub struct MigrateMsg {}
