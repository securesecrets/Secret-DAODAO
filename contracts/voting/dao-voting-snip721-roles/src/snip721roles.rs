use schemars::JsonSchema;
use secret_toolkit::utils::InitCallback;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Snip721RolesInstantiateMsg {
    /// Code ID for snip721 token contract.
    pub code_id: u64,
    /// Code hash for snip721 token contract.
    pub code_hash: String,
    /// Label to use for instantiated snip721 contract.
    pub label: String,
    /// NFT collection name
    pub name: String,
    /// NFT collection symbol
    pub symbol: String,

    /// entropy used for prng seed
    pub entropy: String,

    /// optional privacy configuration for the contract
    pub config: Option<InstantiateConfig>,
}

/// This type represents optional configuration values.
/// All values are optional and have defaults which are more private by default,
/// but can be overridden if necessary
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct InstantiateConfig {
    /// indicates whether the token IDs and the number of tokens controlled by the contract are
    /// public.  If the token supply is private, only minters can view the token IDs and
    /// number of tokens controlled by the contract
    /// default: False
    pub public_token_supply: Option<bool>,
    /// indicates whether token ownership is public or private.  A user can still change whether the
    /// ownership of their tokens is public or private
    /// default: False
    pub public_owner: Option<bool>,
    /// indicates whether sealed metadata should be enabled.  If sealed metadata is enabled, the
    /// private metadata is not viewable by anyone, not even the owner, until the owner calls the
    /// Reveal function.  When Reveal is called, the sealed metadata is irreversibly moved to the
    /// public metadata (as default).  if unwrapped_metadata_is_private is set to true, it will
    /// remain as private metadata, but the owner will now be able to see it.  Anyone will be able
    /// to query the token to know that it has been unwrapped.  This simulates buying/selling a
    /// wrapped card that no one knows which card it is until it is unwrapped. If sealed metadata
    /// is not enabled, all tokens are considered unwrapped
    /// default:  False
    pub enable_sealed_metadata: Option<bool>,
    /// indicates if the Reveal function should keep the sealed metadata private after unwrapping
    /// This config value is ignored if sealed metadata is not enabled
    /// default: False
    pub unwrapped_metadata_is_private: Option<bool>,
    /// indicates whether a minter is permitted to update a token's metadata
    /// default: True
    pub minter_may_update_metadata: Option<bool>,
    /// indicates whether the owner of a token is permitted to update a token's metadata
    /// default: False
    pub owner_may_update_metadata: Option<bool>,
    /// Indicates whether burn functionality should be enabled
    /// default: False
    pub enable_burn: Option<bool>,
}

impl Default for InstantiateConfig {
    fn default() -> Self {
        InstantiateConfig {
            public_token_supply: Some(false),
            public_owner: Some(false),
            enable_sealed_metadata: Some(false),
            unwrapped_metadata_is_private: Some(false),
            minter_may_update_metadata: Some(true),
            owner_may_update_metadata: Some(false),
            enable_burn: Some(false),
        }
    }
}

impl InitCallback for Snip721RolesInstantiateMsg {
    const BLOCK_SIZE: usize = 264;
}
