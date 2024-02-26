use cosmwasm_schema::{cw_serde, QueryResponses};
use cw4::Member;
use secret_cw_controllers::HookItem;

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
        at_height: Option<u64>,
    },
    /// Shows all registered hooks.
    #[returns(secret_cw_controllers::HooksResponse)]
    Hooks {},
}
