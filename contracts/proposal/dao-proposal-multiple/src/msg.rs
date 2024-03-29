use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Api, StdResult};
use dao_dao_macros::proposal_module_query;
use dao_voting::{
    multiple_choice::{MultipleChoiceOptions, MultipleChoiceVote, VotingStrategy},
    pre_propose::PreProposeInfo,
    veto::VetoConfig,
};
use secret_toolkit::permit::Permit;
use secret_utils::Duration;

#[cw_serde]
pub struct InstantiateMsg {
    /// Voting params configuration
    pub voting_strategy: VotingStrategy,
    /// The minimum amount of time a proposal must be open before
    /// passing. A proposal may fail before this amount of time has
    /// elapsed, but it will not pass. This can be useful for
    /// preventing governance attacks wherein an attacker aquires a
    /// large number of tokens and forces a proposal through.
    pub min_voting_period: Option<Duration>,
    /// The amount of time a proposal can be voted on before expiring
    pub max_voting_period: Duration,
    /// If set to true only members may execute passed
    /// proposals. Otherwise, any address may execute a passed
    /// proposal.
    pub only_members_execute: bool,
    /// Allows changing votes before the proposal expires. If this is
    /// enabled proposals will not be able to complete early as final
    /// vote information is not known until the time of proposal
    /// expiration.
    pub allow_revoting: bool,
    /// Information about what addresses may create proposals.
    pub pre_propose_info: PreProposeInfo,
    /// If set to true proposals will be closed if their execution
    /// fails. Otherwise, proposals will remain open after execution
    /// failure. For example, with this enabled a proposal to send 5
    /// tokens out of a DAO's treasury with 4 tokens would be closed when
    /// it is executed. With this disabled, that same proposal would
    /// remain open until the DAO's treasury was large enough for it to be
    /// executed.
    pub close_proposal_on_execution_failure: bool,
    /// Optional veto configuration for proposal execution.
    /// If set, proposals can only be executed after the timelock
    /// delay expiration.
    /// During this period an oversight account (`veto.vetoer`) can
    /// veto the proposal.
    pub veto: Option<VetoConfig>,

    // dao code hash
    pub dao_code_hash: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Creates a proposal in the governance module.
    Propose {
        /// The title of the proposal.
        title: String,
        /// A description of the proposal.
        description: String,
        /// The multiple choices.
        choices: MultipleChoiceOptions,
        /// The address creating the proposal. If no pre-propose
        /// module is attached to this module this must always be None
        /// as the proposer is the sender of the propose message. If a
        /// pre-propose module is attached, this must be Some and will
        /// set the proposer of the proposal it creates.
        proposer: Option<String>,
    },
    /// Votes on a proposal. Voting power is determined by the DAO's
    /// voting power module.
    Vote {
        /// Viewng key of the sender
        key: String,
        /// The ID of the proposal to vote on.
        proposal_id: u64,
        /// The senders position on the proposal.
        vote: MultipleChoiceVote,
        /// An optional rationale for why this vote was cast. This can
        /// be updated, set, or removed later by the address casting
        /// the vote.
        rationale: Option<String>,
    },
    /// Causes the messages associated with a passed proposal to be
    /// executed by the DAO.
    Execute {
        /// Viewng key of the sender
        key: String,
        /// The ID of the proposal to execute.
        proposal_id: u64,
    },
    /// Callable only if veto is configured
    Veto {
        /// The ID of the proposal to veto.
        proposal_id: u64,
    },
    /// Closes a proposal that has failed (either not passed or timed
    /// out). If applicable this will cause the proposal deposit
    /// associated wth said proposal to be returned.
    Close {
        /// The ID of the proposal to close.
        proposal_id: u64,
    },
    /// Updates the governance module's config.
    UpdateConfig {
        /// The new proposal voting strategy. This will only apply
        /// to proposals created after the config update.
        voting_strategy: VotingStrategy,
        /// The minimum amount of time a proposal must be open before
        /// passing. A proposal may fail before this amount of time has
        /// elapsed, but it will not pass. This can be useful for
        /// preventing governance attacks wherein an attacker aquires a
        /// large number of tokens and forces a proposal through.
        min_voting_period: Option<Duration>,
        /// The default maximum amount of time a proposal may be voted
        /// on before expiring. This will only apply to proposals
        /// created after the config update.
        max_voting_period: Duration,
        /// If set to true only members may execute passed
        /// proposals. Otherwise, any address may execute a passed
        /// proposal. Applies to all outstanding and future proposals.
        only_members_execute: bool,
        /// Allows changing votes before the proposal expires. If this is
        /// enabled proposals will not be able to complete early as final
        /// vote information is not known until the time of proposal
        /// expiration.
        allow_revoting: bool,
        /// The address if tge DAO that this governance module is
        /// associated with.
        dao: String,
        /// The code hash if tge DAO that this governance module is
        /// associated with.
        code_hash: String,
        /// If set to true proposals will be closed if their execution
        /// fails. Otherwise, proposals will remain open after execution
        /// failure. For example, with this enabled a proposal to send 5
        /// tokens out of a DAO's treasury with 4 tokens would be closed when
        /// it is executed. With this disabled, that same proposal would
        /// remain open until the DAO's treasury was large enough for it to be
        /// executed.
        close_proposal_on_execution_failure: bool,
        /// Optional time delay on proposal execution, during which the
        /// proposal may be vetoed.
        veto: Option<VetoConfig>,
    },
    /// Updates the sender's rationale for their vote on the specified
    /// proposal. Errors if no vote vote has been cast.
    UpdateRationale {
        proposal_id: u64,
        rationale: Option<String>,
    },
    /// Update's the proposal creation policy used for this
    /// module. Only the DAO may call this method.
    UpdatePreProposeInfo {
        info: PreProposeInfo,
    },
    AddProposalHook {
        address: String,
        code_hash: String,
    },
    RemoveProposalHook {
        address: String,
        code_hash: String,
    },
    AddVoteHook {
        address: String,
        code_hash: String,
    },
    RemoveVoteHook {
        address: String,
        code_hash: String,
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

#[proposal_module_query]
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Gets the governance module's config.
    #[returns(crate::state::Config)]
    Config {},
    /// Gets information about a proposal.
    #[returns(crate::query::ProposalResponse)]
    Proposal { proposal_id: u64 },
    /// Lists all the proposals that have been cast in this module.
    #[returns(crate::query::ProposalListResponse)]
    ListProposals {
        start_after: Option<u64>,
        limit: Option<u64>,
    },
    /// Lists all of the proposals that have been cast in this module
    /// in decending order of proposal ID.
    #[returns(crate::query::ProposalListResponse)]
    ReverseProposals {
        start_before: Option<u64>,
        limit: Option<u64>,
    },
    /// Returns a voters position on a proposal.
    #[returns(crate::query::VoteResponse)]
    GetVote {
        proposal_id: u64,
        voter: String,
        key: String,
    },
    /// Lists all of the votes that have been cast on a proposal.
    #[returns(crate::query::VoteListResponse)]
    ListVotes {
        proposal_id: u64,
        start_after: Option<String>,
        limit: Option<u64>,
    },
    /// Returns the number of proposals that have been created in this module.
    #[returns(::std::primitive::u64)]
    ProposalCount {},
    /// Gets the current proposal creation policy for this module.
    #[returns(::dao_voting::pre_propose::ProposalCreationPolicy)]
    ProposalCreationPolicy {},
    /// Lists all of the consumers of proposal hooks for this module.
    #[returns(::cw_hooks::HooksResponse)]
    ProposalHooks {},
    /// Lists all of the consumers of vote hooks for this module.
    #[returns(::cw_hooks::HooksResponse)]
    VoteHooks {},
    #[returns(())]
    WithPermit {
        permit: Permit,
        query: QueryWithPermit,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryWithPermit {
    #[returns(crate::query::VoteResponse)]
    GetVote { proposal_id: u64, voter: String },
}

impl QueryMsg {
    pub fn get_validation_params(&self, api: &dyn Api) -> StdResult<(Vec<Addr>, String)> {
        match self {
            Self::GetVote { voter, key, .. } => {
                let address = api.addr_validate(voter.as_str())?;
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
pub struct VoteMsg {
    pub proposal_id: u64,
    pub vote: MultipleChoiceVote,
}

#[cw_serde]
pub enum MigrateMsg {
    FromV1 {
        /// This field was not present in DAO DAO v1. To migrate, a
        /// value must be specified.
        ///
        /// If set to true proposals will be closed if their execution
        /// fails. Otherwise, proposals will remain open after execution
        /// failure. For example, with this enabled a proposal to send 5
        /// tokens out of a DAO's treasury with 4 tokens would be closed when
        /// it is executed. With this disabled, that same proposal would
        /// remain open until the DAO's treasury was large enough for it to be
        /// executed.
        close_proposal_on_execution_failure: bool,
        /// This field was not present in DAO DAO v1. To migrate, a
        /// value must be specified.
        ///
        /// This contains information about how a pre-propose module may be configured.
        /// If set to "AnyoneMayPropose", there will be no pre-propose module and consequently,
        /// no deposit or membership checks when submitting a proposal. The "ModuleMayPropose"
        /// option allows for instantiating a prepropose module which will handle deposit verification and return logic.
        pre_propose_info: PreProposeInfo,
        /// This field was not present in DAO DAO v1. To migrate, a
        /// value must be specified.
        ///
        /// optional configuration for veto feature
        veto: Option<VetoConfig>,
    },
    FromCompatible {},
}
