use cosmwasm_schema::cw_serde;
use cw4::Member;
use secret_toolkit::utils::InitCallback;
use shade_protocol::utils::asset::RawContract;

#[cw_serde]
pub struct Cw4GroupInstantiateMsg {
    /// The admin is the only account that can update the group state.
    /// Omit it to make the group immutable.
    pub admin: Option<String>,
    pub members: Vec<Member>,
    pub query_auth: RawContract,
    pub voting_code_hash: Option<String>,
}

impl InitCallback for Cw4GroupInstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}
