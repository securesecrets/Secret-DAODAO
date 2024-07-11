use cosmwasm_schema::cw_serde;
use dao_voting::threshold::PercentageThreshold;
use secret_toolkit::utils::InitCallback;
use secret_utils::Duration;
use dao_voting::{multiple_choice::VotingStrategy, pre_propose::PreProposeInfo, veto::VetoConfig,threshold::Threshold};
use shade_protocol::utils::asset::RawContract;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cosmwasm_std::{Binary, Uint128};
use dao_voting::threshold::ActiveThreshold;
use dao_snip721_extensions::roles::MetadataExt;
use snip721_roles_impl::{
    msg::{InstantiateConfig, PostInstantiateCallback},
    royalties::RoyaltyInfo,
};

pub type ProposalCondorcetInstantiateMsg = UncheckedConfig;

#[cw_serde]
pub struct UncheckedConfig {
    pub quorum: PercentageThreshold,
    pub voting_period: Duration,
    pub min_voting_period: Option<Duration>,
    pub close_proposals_on_execution_failure: bool,
    pub dao_code_hash: String,
}

impl InitCallback for ProposalCondorcetInstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}


#[cw_serde]
pub struct ProposalMultipleInstantiateMsg {
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

    pub query_auth: Option<RawContract>,
}

impl InitCallback for ProposalMultipleInstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}


#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct ProposalSingleInstantiateMsg {
    /// The threshold a proposal must reach to complete.
    pub threshold: Threshold,
    /// The default maximum amount of time a proposal may be voted on
    /// before expiring.
    pub max_voting_period: Duration,
    /// The minimum amount of time a proposal must be open before
    /// passing. A proposal may fail before this amount of time has
    /// elapsed, but it will not pass. This can be useful for
    /// preventing governance attacks wherein an attacker aquires a
    /// large number of tokens and forces a proposal through.
    pub min_voting_period: Option<Duration>,
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
    /// Code hash of dao
    pub dao_code_hash: String,

    pub query_auth: Option<RawContract>,
}

impl InitCallback for ProposalSingleInstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}


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
pub struct VotingCW4nstantiateMsg {
    pub group_contract: GroupContract,
    pub dao_code_hash: String,
}

impl InitCallback for VotingCW4nstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}


/// Information about the staking contract to be used with this voting
/// module.
#[cw_serde]
pub enum StakingInfo {
    Existing {
        /// Address of an already instantiated staking contract.
        staking_contract_address: String,
        /// code hash of an already instantiated staking contract.
        staking_contract_code_hash: String,
    },
    New {
        /// Code ID for staking contract to instantiate.
        staking_code_id: u64,
        /// Code hash for staking contract to instantiate.
        staking_code_hash: String,
        /// label for the contract
        label: String,
        /// See corresponding field in cw20-stake's
        /// instantiation. This will be used when instantiating the
        /// new staking contract.
        unstaking_duration: Option<Duration>,
    },
}

#[cw_serde]
#[allow(clippy::large_enum_variant)]
pub enum Snip20TokenInfo {
    Existing {
        /// Address of an already instantiated cw20 token contract.
        address: String,
        /// Code hash of an already instantiated cw20 token contract.
        code_hash: String,
        /// Information about the staking contract to use.
        staking_contract: StakingInfo,
    },
    New {
        /// Code ID for snip20 token contract.
        code_id: u64,
        /// Code hash for snip20 token contract
        code_hash: String,
        name: String,
        symbol: String,
        decimals: u8,
        initial_balances: Vec<InitialBalance>,

        staking_code_id: u64,
        staking_code_hash: String,
        unstaking_duration: Option<Duration>,
        initial_dao_balance: Option<Uint128>,
    },
}

#[cw_serde]
pub struct Snip20StakedInstantiateMsg {
    pub token_info: Snip20TokenInfo,
    /// The number or percentage of tokens that must be staked
    /// for the DAO to be active
    pub active_threshold: Option<ActiveThreshold>,
    pub dao_code_hash: String,
    pub query_auth: Option<RawContract>,
}

#[cw_serde]
pub struct InitialBalance {
    pub address: String,
    pub amount: Uint128,
}

impl InitCallback for Snip20StakedInstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}


#[cw_serde]
pub struct NftMintMsg {
    /// Unique ID of the NFT
    pub token_id: String,
    /// The owner of the newly minter NFT
    pub owner: String,
    /// Universal resource identifier for this NFT
    /// Should point to a JSON file that conforms to the ERC721
    /// Metadata JSON Schema
    pub token_uri: Option<String>,
    /// Any custom extension used by this contract
    pub extension: MetadataExt,
}

#[allow(clippy::large_enum_variant)]
#[cw_serde]
pub enum NftRolesContract {
    Existing {
        /// Address of an already instantiated snip721-weighted-roles token contract.
        address: String,
        /// code hash of an already instantiated snip721-weighted-roles token contract.
        code_hash: String,
    },
    New {
        /// Code ID for snip721 roles  contract.
        snip721_roles_code_id: u64,
        /// Code hash for snip721 roles  contract.
        snip721_roles_code_hash: String,
        /// Initial NFTs to mint when instantiating the new cw721 contract.
        /// If empty, an error is thrown.
        initial_nfts: Vec<NftMintMsg>,
        /// name of token contract
        name: String,
        /// token contract symbol
        symbol: String,
        /// optional admin address, env.message.sender if missing
        admin: Option<String>,
        /// entropy used for prng seed
        entropy: String,
        /// optional royalty information to use as default when RoyaltyInfo is not provided to a
        /// minting function
        royalty_info: Option<RoyaltyInfo>,
        /// optional privacy configuration for the contract
        config: Option<InstantiateConfig>,
        /// optional callback message to execute after instantiation.  This will
        /// most often be used to have the token contract provide its address to a
        /// contract that instantiated it, but it could be used to execute any
        /// contract
        post_init_callback: Option<PostInstantiateCallback>,
        query_auth: Option<RawContract>,
    },
}

#[cw_serde]
pub struct Snip721RolesInstantiateMsg {
    /// Info about the associated NFT contract
    pub nft_contract: NftRolesContract,
    pub dao_code_hash: String,
}

impl InitCallback for Snip721RolesInstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}


#[cw_serde]
#[allow(clippy::large_enum_variant)]
pub enum NftContract {
    /// Uses an existing snip721 or sg721 token contract.
    Existing {
        /// Address of an already instantiated snip721 or sg721 token contract.
        address: String,
        /// code hash of an already instantiated snip721 or sg721 token contract.
        code_hash: String,
    },
    /// Creates a new NFT collection used for staking and governance.
    New {
        /// Code ID for snip721 token contract.
        code_id: u64,
        /// Code hash for snip721 token contract.
        code_hash: String,
        /// Label to use for instantiated cw721 contract.
        label: String,
        msg: Binary,
        /// Initial NFTs to mint when creating the NFT contract.
        /// If empty, an error is thrown. The binary should be a
        /// valid mint message for the corresponding cw721 contract.
        initial_nfts: Vec<Binary>,
    },
    /// Uses a factory contract that must return the address of the NFT contract.
    /// The binary must serialize to a `WasmMsg::Execute` message.
    /// Validation happens in the factory contract itself, so be sure to use a
    /// trusted factory contract.
    Factory(Binary),
}

#[cw_serde]
pub struct Snip721StakedInstantiateMsg {
    /// Address of the cw721 NFT contract that may be staked.
    pub nft_contract: NftContract,
    /// Amount of time between unstaking and tokens being
    /// avaliable. To unstake with no delay, leave as `None`.
    pub unstaking_duration: Option<Duration>,
    /// The number or percentage of tokens that must be staked
    /// for the DAO to be active
    pub active_threshold: Option<ActiveThreshold>,

    pub dao_code_hash: String,

    pub query_auth: Option<RawContract>,
}

impl InitCallback for Snip721StakedInstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}


#[cw_serde]
pub struct TokenStakedInstantiateMsg {
    /// New or existing native token to use for voting power.
    pub token_info: TokenInfo,
    /// How long until the tokens become liquid again
    pub unstaking_duration: Option<Duration>,
    /// The number or percentage of tokens that must be staked
    /// for the DAO to be active
    pub active_threshold: Option<ActiveThreshold>,
    pub dao_code_hash: String,
    pub query_auth: Option<RawContract>,
}

#[cw_serde]
pub enum TokenInfo {
    /// Uses an existing Token Factory token and creates a new issuer contract.
    /// Full setup, such as transferring ownership or setting up MsgSetBeforeSendHook,
    /// must be done manually.
    Existing {
        /// Token factory denom
        denom: String,
    },
}

impl InitCallback for TokenStakedInstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}
