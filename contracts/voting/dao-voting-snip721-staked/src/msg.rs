use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary};
use dao_dao_macros::{active_query, voting_module_query};
use dao_voting::threshold::{ActiveThreshold, ActiveThresholdResponse};
use schemars::JsonSchema;
use secret_utils::Duration;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Snip721ReceiveMsg {
    /// ReceiveNft may be a HandleMsg variant of any contract that wants to implement a receiver
    /// interface.  BatchReceiveNft, which is more informative and more efficient, is preferred over
    /// ReceiveNft.  Please read above regarding why ReceiveNft, which follows CW-721 standard has an
    /// inaccurately named `sender` field
    ReceiveNft {
        /// previous owner of sent token
        sender: Addr,
        /// token that was sent
        token_id: String,
        /// optional message to control receiving logic
        msg: Option<Binary>,
    },
    /// BatchReceiveNft may be a HandleMsg variant of any contract that wants to implement a receiver
    /// interface.  BatchReceiveNft, which is more informative and more efficient, is preferred over
    /// ReceiveNft.
    BatchReceiveNft {
        /// address that sent the tokens.  There is no ReceiveNft field equivalent to this
        sender: Addr,
        /// previous owner of sent tokens.  This is equivalent to the ReceiveNft `sender` field
        from: Addr,
        /// tokens that were sent
        token_ids: Vec<String>,
        /// optional message to control receiving logic
        msg: Option<Binary>,
    },
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
pub struct InstantiateMsg {
    /// Address of the cw721 NFT contract that may be staked.
    pub nft_contract: NftContract,
    /// Amount of time between unstaking and tokens being
    /// avaliable. To unstake with no delay, leave as `None`.
    pub unstaking_duration: Option<Duration>,
    /// The number or percentage of tokens that must be staked
    /// for the DAO to be active
    pub active_threshold: Option<ActiveThreshold>,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Used to stake NFTs. To stake a NFT send a cw721 send message
    /// to this contract with the NFT you would like to stake. The
    /// `msg` field is ignored.
    ReceiveNft(Snip721ReceiveMsg),
    /// Unstakes the specified token_ids on behalf of the
    /// sender. token_ids must have unique values and have non-zero
    /// length.
    Unstake { token_ids: Vec<String> },
    /// Claim NFTs that have been unstaked for the specified duration.
    ClaimNfts {},
    /// Updates the contract configuration, namely unstaking duration.
    /// Only callable by the DAO that initialized this voting contract.
    UpdateConfig { duration: Option<Duration> },
    /// Adds a hook which is called on staking / unstaking events.
    /// Only callable by the DAO that initialized this voting contract.
    AddHook { addr: String, code_hash: String },
    /// Removes a hook which is called on staking / unstaking events.
    /// Only callable by the DAO that initialized this voting contract.
    RemoveHook { addr: String, code_hash: String },
    /// Sets the active threshold to a new value.
    /// Only callable by the DAO that initialized this voting contract.
    UpdateActiveThreshold {
        new_threshold: Option<ActiveThreshold>,
    },
}

#[active_query]
#[voting_module_query]
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(crate::state::Config)]
    Config {},
    #[returns(::snip721_controllers::NftClaimsResponse)]
    NftClaims { address: String },
    #[returns(::secret_cw_controllers::HooksResponse)]
    Hooks {},
    // List the staked NFTs for a given address.
    #[returns(Vec<String>)]
    StakedNfts { address: String },
    #[returns(ActiveThresholdResponse)]
    ActiveThreshold {},
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub struct IsActiveResponse {
    pub active: bool,
}
