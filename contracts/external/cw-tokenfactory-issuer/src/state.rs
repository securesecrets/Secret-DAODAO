use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use secret_storage_plus::Item;
use secret_toolkit::storage::Keymap;

/// Holds the Token Factory denom managed by this contract
pub const DENOM: Item<String> = Item::new("denom");

/// Denylist addresses prevented from transferring tokens
pub const DENYLIST: Keymap<Addr, bool> = Keymap::new(b"denylist");

/// Addresses allowed to transfer tokens even if the token is frozen
pub const ALLOWLIST: Keymap<Addr, bool> = Keymap::new(b"allowlist");

/// Whether or not features that require MsgBeforeSendHook are enabled
/// Many Token Factory chains do not yet support MsgBeforeSendHook
#[cw_serde]
pub struct BeforeSendHookInfo {
    /// Whether or not features in this contract that require MsgBeforeSendHook are enabled.
    pub advanced_features_enabled: bool,
    /// The address of the contract that implements the BeforeSendHook interface.
    /// Most often this will be the cw_tokenfactory_issuer contract itself.
    pub hook_contract_address: Option<String>,
}
pub const BEFORE_SEND_HOOK_INFO: Item<BeforeSendHookInfo> = Item::new("hook_features_enabled");

/// Whether or not token transfers are frozen
pub const IS_FROZEN: Item<bool> = Item::new("is_frozen");

/// Allowances for burning
pub const BURNER_ALLOWANCES: Keymap<Addr, Uint128> = Keymap::new(b"burner_allowances");

/// Allowances for minting
pub const MINTER_ALLOWANCES: Keymap<Addr, Uint128> = Keymap::new(b"minter_allowances");
