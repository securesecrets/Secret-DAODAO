use cosmwasm_schema::cw_serde;
use dao_voting::threshold::ActiveThreshold;
use secret_toolkit::utils::InitCallback;
use secret_utils::Duration;
use shade_protocol::utils::asset::RawContract;

#[cw_serde]
pub struct InstantiateMsg {
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
    // NOTE* There is right now no way to create new token so will be using existing token

    // Creates a new Token Factory token via the issue contract with the DAO automatically
    // setup as admin and owner.

    // New(NewTokenInfo),

    // Uses a factory contract that must return the denom, optionally a Token Contract address.
    // The binary must serialize to a `WasmMsg::Execute` message.
    // Validation happens in the factory contract itself, so be sure to use a
    // trusted factory contract.
    // Factory(Binary),
}

impl InitCallback for InstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}
