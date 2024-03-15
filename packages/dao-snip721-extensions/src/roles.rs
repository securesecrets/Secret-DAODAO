use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Api, CustomMsg, StdResult};
use secret_toolkit::permit::Permit;

#[cw_serde]
pub struct MetadataExt {
    /// Optional on-chain role for this member, can be used by other contracts to enforce permissions
    pub role: Option<String>,
    /// The voting weight of this role
    pub weight: u64,
}

#[cw_serde]
pub enum ExecuteExt {
    /// Add a new hook to be informed of all membership changes.
    /// Must be called by Admin
    AddHook { addr: String, code_hash: String },
    /// Remove a hook. Must be called by Admin
    RemoveHook { addr: String, code_hash: String },
    /// Update the token_uri for a particular NFT. Must be called by minter / admin
    UpdateTokenUri {
        token_id: String,
        token_uri: Option<String>,
    },
    /// Updates the voting weight of a token. Must be called by minter / admin
    UpdateTokenWeight { token_id: String, weight: u64 },
    /// Udates the role of a token. Must be called by minter / admin
    UpdateTokenRole {
        token_id: String,
        role: Option<String>,
    },
    CreateViewingKey {
        entropy: String,
        padding: Option<String>,
    },
    SetViewingKey {
        key: String,
        padding: Option<String>,
    },
    // Permit
    RevokePermit {
        permit_name: String,
        padding: Option<String>,
    },
}
impl CustomMsg for ExecuteExt {}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryExt {
    /// Total weight at a given height
    #[returns(cw4::TotalWeightResponse)]
    TotalWeight { at_height: Option<u64> },
    /// Returns a list of Members
    #[returns(cw4::MemberListResponse)]
    ListMembers {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns the weight of a certain member
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

impl QueryExt {
    pub fn get_validation_params(&self, api: &dyn Api) -> StdResult<(Vec<Addr>, String)> {
        match self {
            Self::Member { addr, key, .. } => {
                let address = api.addr_validate(addr.as_str())?;
                Ok((vec![address], key.clone()))
            }
            _ => panic!("This query type does not require authentication"),
        }
    }
}

#[cw_serde]
pub struct CreateViewingKey {
    pub key: String,
}

#[cw_serde]
pub struct ViewingKeyError {
    pub msg: String,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryWithPermit {
    #[returns(dao_interface::voting::VotingPowerAtHeightResponse)]
    Member {
        addr: String,
        at_height: Option<u64>,
    },
}

impl CustomMsg for QueryExt {}
