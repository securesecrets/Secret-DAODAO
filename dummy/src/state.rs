use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cw_hooks::Hooks;
use cosmwasm_std::{Addr, Storage,Uint128};
use secret_toolkit::storage::{Item,Keymap};
use secret_utils::Duration;
use secret_cw_controllers::Claims;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Config {
    pub token_address: Addr,
    pub unstaking_duration: Option<Duration>,
}


pub const CONFIG: Item<Config> = Item::new(b"config_v2");
pub const STAKED_TOTAL: Item<Uint128> = Item::new(b"total_staked");
pub const BALANCE: Item<Uint128> = Item::new(b"balance");
pub const STAKED_BALANCES: Keymap<Addr, Uint128> = Keymap::new(b"staked_balances");


// Hooks to contracts that will receive staking and unstaking messages
pub const HOOKS: Hooks = Hooks::new("hooks");

/// The maximum number of claims that may be outstanding.
pub const MAX_CLAIMS: u64 = 100;

pub const CLAIMS: Claims = Claims::new("claims");