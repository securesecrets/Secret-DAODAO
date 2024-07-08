use cosmwasm_schema::{cw_serde, QueryResponses};
use cw4::Member;
use secret_cw_controllers::HookItem;
use shade_protocol::{basic_staking::Auth, utils::asset::RawContract};

#[cw_serde]
pub struct InstantiateMsg {
    /// The admin is the only account that can update the group state.
    /// Omit it to make the group immutable.
    pub admin: Option<String>,
    pub members: Vec<Member>,
    pub query_auth: RawContract,
    // Set voting code_hash to none is not deploying from voting module
    pub voting_code_hash: Option<String>,
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
#[allow(clippy::large_enum_variant)]
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
    Member { auth: Auth, at_height: Option<u64> },
    /// Shows all registered hooks.
    #[returns(secret_cw_controllers::HooksResponse)]
    Hooks {},
}
