use cosmwasm_std::{Addr, Storage, Uint128};
use cw_hooks::Hooks;
use schemars::JsonSchema;
use secret_cw_controllers::Claims;
use secret_storage_plus::{Item, Map};
use secret_toolkit::storage::Keymap;
use secret_utils::Duration;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Config {
    pub token_address: Addr,
    pub token_code_hash: String,
    pub unstaking_duration: Option<Duration>,
}

pub const RESPONSE_BLOCK_SIZE: usize = 256;
pub const CONFIG: Item<Config> = Item::new("config");
pub const BALANCE: Item<Uint128> = Item::new("balance");

// Hooks to contracts that will receive staking and unstaking messages
pub const HOOKS: Hooks = Hooks::new("hooks");

/// The maximum number of claims that may be outstanding.
pub const MAX_CLAIMS: u64 = 100;

pub const CLAIMS: Claims = Claims::new("claims");

pub const STAKED_TOTAL: Map<u64, Uint128> = Map::new("total_staked");
pub struct StakedTotalStore {}
impl StakedTotalStore {
    // Function to store a value at a specific block height
    pub fn store_staked_total_at_blockheight(
        store: &mut dyn Storage,
        block_height: u64,
        value: Uint128,
    ) {
        // Store at the specific block height
        let _ = STAKED_TOTAL.save(store, block_height, &value);

        // Also store without specifying a block height
        let _ = STAKED_TOTAL.save(store, 0u64, &value);
    }

    // Function to load a value at a specific block height, or the latest if not provided
    pub fn load_staked_total(store: &dyn Storage, block_height: Option<u64>) -> Uint128 {
        // Check if a specific block height is provided
        if let Some(height) = block_height {
            return STAKED_TOTAL.load(store, height).unwrap_or_default();
        }

        // If no specific block height is provided, load the latest value
        STAKED_TOTAL.load(store, 0u64).unwrap_or_default()
    }
}

pub static STAKED_BALANCES: Keymap<(u64, Addr), Uint128> = Keymap::new(b"staked_balances");
pub struct StakedBalancesStore {}
impl StakedBalancesStore {
    // Function to store a value at a specific block height
    pub fn store_staked_balance_at_blockheight(
        store: &mut dyn Storage,
        block_height: u64,
        addr: Addr,
        value: Uint128,
    ) {
        // Store at the specific block height
        let _ = STAKED_BALANCES.insert(store, &(block_height, addr.clone()), &value);

        // Also store without specifying a block height
        let _ = STAKED_BALANCES.insert(store, &(0u64, addr.clone()), &value);
    }

    // Function to load a value at a specific block height, or the latest if not provided
    pub fn load_staked_balance(
        store: &dyn Storage,
        addr: Addr,
        block_height: Option<u64>,
    ) -> Uint128 {
        // Check if a specific block height is provided
        if let Some(height) = block_height {
            return STAKED_BALANCES
                .get(store, &(height, addr))
                .unwrap_or_default();
        }

        // If no specific block height is provided, load the latest value
        STAKED_BALANCES
            .get(store, &(0u64, addr))
            .unwrap_or_default()
    }
}
