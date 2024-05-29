use cosmwasm_schema::cw_serde;
use cosmwasm_std::Binary;
use dao_voting::threshold::ActiveThreshold;
use secret_toolkit::utils::InitCallback;
use secret_utils::Duration;
use shade_protocol::utils::asset::RawContract;

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

    pub dao_code_hash: String,

    pub query_auth: Option<RawContract>,
}

impl InitCallback for InstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}
