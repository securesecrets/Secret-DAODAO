use cosmwasm_schema::{cw_serde, QueryResponses};
use cw4::Member;
use secret_toolkit::utils::InitCallback;
use shade_protocol::{basic_staking::Auth, utils::asset::RawContract};

#[cw_serde]
pub struct Cw4GroupInstantiateMsg {
    /// The admin is the only account that can update the group state.
    /// Omit it to make the group immutable.
    pub admin: Option<String>,
    pub members: Vec<Member>,
    pub query_auth: RawContract,
}

impl InitCallback for Cw4GroupInstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}

#[cw_serde]
#[derive(QueryResponses)]
#[allow(clippy::large_enum_variant)]
pub enum Cw4GroupQueryMsg {
    #[returns(cw4::TotalWeightResponse)]
    TotalWeight { at_height: Option<u64> },
    #[returns(cw4::MemberListResponse)]
    ListMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    #[returns(cw4::MemberResponse)]
    Member { auth: Auth, at_height: Option<u64> },
}
