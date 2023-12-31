use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cw_hooks::Hooks;
use cosmwasm_std::{Addr, Storage,Uint128};
use secret_storage_plus::{Item,Map};
use secret_utils::Duration;
use secret_cw_controllers::Claims;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Config {
    pub token_address: Addr,
    pub unstaking_duration: Option<Duration>,
}


pub const CONFIG: Item<Config> = Item::new("config_v2");
pub const STAKED_TOTAL: Item<Uint128> = Item::new("total_staked");
pub const BALANCE: Item<Uint128> = Item::new("balance");
pub const STAKED_BALANCES: Map<Addr, Uint128> = Map::new("staked_balances");


// Hooks to contracts that will receive staking and unstaking messages
pub const HOOKS: Hooks = Hooks::new("hooks");

/// The maximum number of claims that may be outstanding.
pub const MAX_CLAIMS: u64 = 100;

pub const CLAIMS: Claims = Claims::new("claims");