use cosmwasm_schema::{schemars::JsonSchema, QueryResponses};
use cw_denom::UncheckedDenom;
use dao_voting::{
    deposit::{CheckedDepositInfo, UncheckedDepositInfo},
    status::Status,
};
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg<InstantiateExt> {
    /// Information about the deposit requirements for this
    /// module. None if no deposit.
    pub deposit_info: Option<UncheckedDepositInfo>,
    /// If false, only members (addresses with voting power) may create
    /// proposals in the DAO. Otherwise, any address may create a
    /// proposal so long as they pay the deposit.
    pub open_proposal_submission: bool,
    pub proposal_module_code_hash: String,
    /// Extension for instantiation. The default implementation will
    /// do nothing with this data.
    pub extension: InstantiateExt,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg<ProposalMessage, ExecuteExt> {
    /// Creates a new proposal in the pre-propose module. MSG will be
    /// serialized and used as the proposal creation message.
    Propose { key: String, 
        msg: ProposalMessage 
    },

    /// Updates the configuration of this module. This will completely
    /// override the existing configuration. This new configuration
    /// will only apply to proposals created after the config is
    /// updated. Only the DAO may execute this message.
    UpdateConfig {
        deposit_info: Option<UncheckedDepositInfo>,
        open_proposal_submission: bool,
    },

    /// Withdraws funds inside of this contract to the message
    /// sender. The contracts entire balance for the specifed DENOM is
    /// withdrawn to the message sender. Only the DAO may call this
    /// method.
    ///
    /// This is intended only as an escape hatch in the event of a
    /// critical bug in this contract or it's proposal
    /// module. Withdrawing funds will cause future attempts to return
    /// proposal deposits to fail their transactions as the contract
    /// will have insufficent balance to return them. In the case of
    /// `cw-proposal-single` this transaction failure will cause the
    /// module to remove the pre-propose module from its proposal hook
    /// receivers.
    ///
    /// More likely than not, this should NEVER BE CALLED unless a bug
    /// in this contract or the proposal module it is associated with
    /// has caused it to stop receiving proposal hook messages, or if
    /// a critical security vulnerability has been found that allows
    /// an attacker to drain proposal deposits.
    Withdraw {
        /// The denom to withdraw funds for. If no denom is specified,
        /// the denomination currently configured for proposal
        /// deposits will be used.
        ///
        /// You may want to specify a denomination here if you are
        /// withdrawing funds that were previously accepted for
        /// proposal deposits but are not longer used due to an
        /// `UpdateConfig` message being executed on the contract.
        denom: Option<UncheckedDenom>,
        key: String,
    },

    /// Extension message. Contracts that extend this one should put
    /// their custom execute logic here. The default implementation
    /// will do nothing if this variant is executed.
    Extension { msg: ExecuteExt },

    /// Adds a proposal submitted hook. Fires when a new proposal is submitted
    /// to the pre-propose contract. Only the DAO may call this method.
    AddProposalSubmittedHook { address: String,code_hash: String },

    /// Removes a proposal submitted hook. Only the DAO may call this method.
    RemoveProposalSubmittedHook { address: String ,code_hash: String},

    /// Handles proposal hook fired by the associated proposal
    /// module when a proposal is completed (ie executed or rejected).
    /// By default, the base contract will return deposits
    /// proposals, when they are closed, when proposals are executed, or,
    /// if it is refunding failed.
    ProposalCompletedHook {
        proposal_id: u64,
        new_status: Status,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
#[derive(QueryResponses)]
pub enum QueryMsg<QueryExt>
where
    QueryExt: JsonSchema,
{
    /// Gets the proposal module that this pre propose module is
    /// associated with. Returns `Addr`.
    #[returns(dao_interface::state::AnyContractInfo)]
    ProposalModule {},
    /// Gets the DAO (dao-dao-core) module this contract is associated
    /// with. Returns `Addr`.
    #[returns(dao_interface::state::AnyContractInfo)]
    Dao {},
    /// Gets the module's configuration.
    #[returns(crate::state::Config)]
    Config {},
    /// Gets the deposit info for the proposal identified by
    /// PROPOSAL_ID.
    #[returns(DepositInfoResponse)]
    DepositInfo { proposal_id: u64 },
    /// Returns list of proposal submitted hooks.
    #[returns(cw_hooks::HooksResponse)]
    ProposalSubmittedHooks {},
    /// Extension for queries. The default implementation will do
    /// nothing if queried for will return `Binary::default()`.
    #[returns(cosmwasm_std::Binary)]
    QueryExtension { msg: QueryExt },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct DepositInfoResponse {
    /// The deposit that has been paid for the specified proposal.
    pub deposit_info: Option<CheckedDepositInfo>,
    /// The address that created the proposal.
    pub proposer: cosmwasm_std::Addr,
}
