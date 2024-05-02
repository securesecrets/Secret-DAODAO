use cosmwasm_schema::{cw_serde, QueryResponses};
use dao_dao_macros::voting_module_query;
use dao_snip721_extensions::roles::MetadataExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use shade_protocol::utils::asset::RawContract;
use snip721_roles_impl::{
    msg::{InstantiateConfig, PostInstantiateCallback},
    royalties::RoyaltyInfo,
};

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
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
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub enum NftContract {
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
        query_auth: RawContract,
    },
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct InstantiateMsg {
    /// Info about the associated NFT contract
    pub nft_contract: NftContract,
    pub dao_code_hash: String,
}

#[cw_serde]
pub enum ExecuteMsg {}

#[allow(clippy::large_enum_variant)]
#[voting_module_query]
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(crate::state::Config)]
    Config {},
}
