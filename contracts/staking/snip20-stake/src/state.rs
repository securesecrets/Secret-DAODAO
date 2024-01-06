use cosmwasm_std::{Addr, Uint128};
use cw_hooks::Hooks;
use schemars::JsonSchema;
use secret_cw_controllers::Claims;
use secret_storage_plus::Item;
use secret_toolkit::storage::Keymap;
use secret_utils::Duration;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Config {
    pub token_address: Addr,
    pub unstaking_duration: Option<Duration>,
}

pub const CONFIG: Item<Config> = Item::new("config_v2");
pub const STAKED_TOTAL: Item<Uint128> = Item::new("total_staked");
pub const BALANCE: Item<Uint128> = Item::new("balance");
pub static  STAKED_BALANCES: Keymap<Addr, Uint128> = Keymap::new(b"staked_balances");

// Hooks to contracts that will receive staking and unstaking messages
pub const HOOKS: Hooks = Hooks::new("hooks");

/// The maximum number of claims that may be outstanding.
pub const MAX_CLAIMS: u64 = 100;

pub const CLAIMS: Claims = Claims::new("claims");
