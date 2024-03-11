use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Api, StdResult};
use cw4::Member;
use schemars::JsonSchema;
use secret_cw_controllers::HookItem;
use secret_toolkit::permit::Permit;
use serde::{Deserialize, Serialize};

#[cw_serde]
pub struct InstantiateMsg {
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
pub enum ExecuteMsg {
    /// Change the admin
    UpdateAdmin { admin: Option<String> },
    /// apply a diff to the existing members.
    /// remove is applied after add, so if an address is in both, it is removed
    UpdateMembers {
        remove: Vec<String>,
        add: Vec<Member>,
    },
    /// Add a new hook to be informed of all membership changes. Must be called by Admin
    AddHook { hook: HookItem },
    /// Remove a hook. Must be called by Admin
    RemoveHook { hook: HookItem },
    CreateViewingKey {
        entropy: String,
        padding: Option<String>,
    },
    SetViewingKey {
        key: String,
        padding: Option<String>,
    }, // Permit
    RevokePermit {
        permit_name: String,
        padding: Option<String>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
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

impl QueryMsg {
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, QueryResponses)]
#[cfg_attr(test, derive(Eq, PartialEq))]
#[serde(rename_all = "snake_case")]
pub enum QueryWithPermit {
    #[returns(cw4::MemberResponse)]
    Member {
        address: String,
        at_height: Option<u64>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct ViewingKeyError {
    pub msg: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct CreateViewingKeyResponse {
    pub key: String,
}
