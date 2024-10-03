use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::CosmosMsg;
use secret_toolkit::utils::InitCallback;

#[cw_serde]
pub struct InstantiateMsg {
    pub root: String,
    pub dao_code_hash: String,
}

impl InitCallback for InstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}

#[cw_serde]
pub enum ExecuteMsg {
    Execute { msgs: Vec<CosmosMsg> },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(cosmwasm_std::Addr)]
    Admin {},
    #[returns(cosmwasm_std::Addr)]
    Dao {},
    #[returns(dao_interface::voting::InfoResponse)]
    Info {},
}
