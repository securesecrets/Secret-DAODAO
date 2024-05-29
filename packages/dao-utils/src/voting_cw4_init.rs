use cosmwasm_schema::cw_serde;
use secret_toolkit::utils::InitCallback;
use shade_protocol::utils::asset::RawContract;

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
        query_auth: Option<RawContract>,
    },
}

#[cw_serde]
pub struct InstantiateMsg {
    pub group_contract: GroupContract,
    pub dao_code_hash: String,
}

impl InitCallback for InstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}
