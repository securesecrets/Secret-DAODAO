use cosmwasm_schema::{cw_serde, QueryResponses};
use cw4::Member;
use schemars::JsonSchema;
use secret_toolkit::{
    permit::Permit,
    utils::{HandleCallback, InitCallback},
};
use serde::{Deserialize, Serialize};

#[cw_serde]
pub struct Cw4GroupInstantiateMsg {
    /// The admin is the only account that can update the group state.
    /// Omit it to make the group immutable.
    pub admin: Option<String>,
    pub members: Vec<Member>,
}

#[cw_serde]
pub struct InstantiateMsgResponse {
    /// The admin is the only account that can update the group state.
    /// Omit it to make the group immutable.
    pub address: String,
    pub code_hash: String,
}

#[cw_serde]
pub enum Cw4GroupExecuteMsg {
    CreateViewingKey {
        entropy: String,
        padding: Option<String>,
    },
}

impl InitCallback for Cw4GroupInstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}

impl HandleCallback for Cw4GroupExecuteMsg {
    const BLOCK_SIZE: usize = 256;
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct CreateViewingKeyResponse {
    pub key: String,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum Cw4GroupQueryMsg {
    #[returns(secret_cw_controllers::AdminResponse)]
    Admin {},
    #[returns(cw4::TotalWeightResponse)]
    TotalWeight { at_height: Option<u64> },
    #[returns(cw4::MemberListResponse)]
    ListMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    #[returns(cw4::MemberResponse)]
    Member {
        addr: String,
        key: String,
        at_height: Option<u64>,
    },
    /// Shows all registered hooks.
    #[returns(secret_cw_controllers::HooksResponse)]
    Hooks {},
    #[returns(())]
    WithPermit {
        permit: Permit,
        query: QueryWithPermit,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryWithPermit {
    #[returns(cw4::MemberResponse)]
    Member {
        address: String,
        at_height: Option<u64>,
    },
}
