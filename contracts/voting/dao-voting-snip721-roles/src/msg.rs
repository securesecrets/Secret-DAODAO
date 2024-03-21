use cosmwasm_schema::{cw_serde, QueryResponses};
use dao_dao_macros::voting_module_query;
use dao_snip721_extensions::roles::MetadataExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub enum NftContract {
    Existing {
        /// Address of an already instantiated snip721-weighted-roles token contract.
        address: String,
        /// code hash of an already instantiated snip721-weighted-roles token contract.
        code_hash: String,
    },
    New {
        /// Code ID for snip721 roles token contract.
        snip721_roles_code_id: u64,
        /// Code hash for snip721 roles token contract.
        snip721_roles_code_hash: String,

        /// Code ID for snip721 token contract.
        snip721_code_id: u64,
        /// Code hash for snip721 token contract.
        snip721_code_hash: String,
        /// Label to use for instantiated snip721 contract.
        label: String,
        /// NFT collection name
        name: String,
        /// NFT collection symbol
        symbol: String,
        /// Initial NFTs to mint when instantiating the new snip721 contract.
        /// If empty, an error is thrown.
        initial_nfts: Vec<NftMintMsg>,

        /// entropy used for prng seed
        entropy: String,
        /// optional privacy configuration for the contract
        config: Option<crate::snip721roles::InstantiateConfig>,
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
