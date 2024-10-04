use cosmwasm_std::{Addr, StdResult, Storage, Uint128};
use cw_hooks::Hooks;
use schemars::JsonSchema;
use secret_cw_controllers::Claims;
use secret_storage_plus::Item;
use secret_toolkit::storage::Keymap;
use secret_utils::Duration;
use serde::{Deserialize, Serialize};
use shade_protocol::Contract;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Config {
    pub token_address: Addr,
    pub token_code_hash: String,
    pub unstaking_duration: Option<Duration>,
    pub query_auth: Contract,
}

pub const RESPONSE_BLOCK_SIZE: usize = 256;
pub const CONFIG: Item<Config> = Item::new("config");
pub const BALANCE: Item<Uint128> = Item::new("balance");

// Hooks to contracts that will receive staking and unstaking messages
pub const HOOKS: Hooks = Hooks::new("hooks");

/// The maximum number of claims that may be outstanding.
pub const MAX_CLAIMS: u64 = 100;

pub const CLAIMS: Claims = Claims::new("claims");

pub const STAKED_TOTAL_AT_HEIGHT: Keymap<u64, Uint128> = Keymap::new(b"staked_total");
pub const TOTAL_BALANCE: Item<Uint128> = Item::new("total_balance");

pub struct StakedTotalStore {}

impl StakedTotalStore {
    // Function to store a value at a specific block height
    pub fn save(store: &mut dyn Storage, block_height: u64, value: Uint128) -> StdResult<()> {
        // Insert the total staked value at the given block height
        STAKED_TOTAL_AT_HEIGHT.insert(store, &block_height, &value)?;
        // Also update the total current balance
        TOTAL_BALANCE.save(store, &value)?;
        Ok(())
    }

    // Load the most recent total staked balance
    pub fn load(store: &dyn Storage) -> Uint128 {
        TOTAL_BALANCE.load(store).unwrap_or_default()
    }

    // Load the staked total at a specific block height, falling back to the most recent total if not found
    pub fn may_load_at_height(store: &dyn Storage, height: u64) -> StdResult<Option<Uint128>> {
        // Check if there is a total staked value recorded at the given height
        if let Some(snapshot_value) = STAKED_TOTAL_AT_HEIGHT.get(store, &height) {
            // If found, return the snapshot value
            Ok(Some(snapshot_value))
        } else {
            // If not found, fallback to the most recent total balance
            let total_balance = TOTAL_BALANCE.load(store)?;
            Ok(Some(total_balance))
        }
    }
}

pub const STAKED_BALANCES_PRIMARY: Keymap<Addr, Uint128> = Keymap::new(b"staked_balances_primary");
pub const STAKED_BALANCES_SNAPSHOT: Keymap<(u64, Addr), Uint128> =
    Keymap::new(b"staked_balances_snapshot");
pub const USER_STAKED_AT_HEIGHT: Keymap<Addr, Vec<u64>> = Keymap::new(b"user_staked_at_height");

pub struct StakedBalancesStore {}

impl StakedBalancesStore {
    // Function to store a value at a specific block height
    pub fn save(
        store: &mut dyn Storage,
        block_height: u64,
        key: Addr,
        value: Uint128,
    ) -> StdResult<()> {
        let primary = STAKED_BALANCES_PRIMARY.get(store, &key);

        if primary.is_none() {
            // First time staking for this user
            STAKED_BALANCES_PRIMARY.insert(store, &key, &value)?;
            STAKED_BALANCES_SNAPSHOT.insert(
                store,
                &(block_height, key.clone()),
                &Uint128::zero(),
            )?;
            USER_STAKED_AT_HEIGHT.insert(store, &key, &vec![block_height])?;
        } else {
            // User has staked before, create a snapshot
            let mut user_staked_height = USER_STAKED_AT_HEIGHT.get(store, &key).unwrap_or_default();
            STAKED_BALANCES_SNAPSHOT.insert(
                store,
                &(block_height, key.clone()),
                &primary.unwrap(),
            )?;
            STAKED_BALANCES_PRIMARY.insert(store, &key, &value)?;

            // Ensure block height is not duplicated
            if !user_staked_height.contains(&block_height) {
                user_staked_height.push(block_height);
            }

            USER_STAKED_AT_HEIGHT.insert(store, &key, &user_staked_height)?;
        }

        Ok(())
    }

    // Load the primary staked balance for the given user
    pub fn load(store: &dyn Storage, key: Addr) -> Uint128 {
        STAKED_BALANCES_PRIMARY.get(store, &key).unwrap_or_default()
    }

    // Load staked balance at a specific height
    pub fn may_load_at_height(
        store: &dyn Storage,
        key: Addr,
        height: u64,
    ) -> StdResult<Option<Uint128>> {
        // Try to get the snapshot value at the given height
        let snapshot_key = (height, key.clone());
        let snapshot_value = STAKED_BALANCES_SNAPSHOT.get(store, &snapshot_key);

        if snapshot_value.is_none() {
            // No snapshot, fallback to the current primary balance
            return Ok(STAKED_BALANCES_PRIMARY.get(store, &key));
        }

        // Get all heights at which the user staked
        let user_staked_heights = USER_STAKED_AT_HEIGHT.get(store, &key).unwrap_or_default();

        // If no staked heights exist, return primary value
        if user_staked_heights.is_empty() {
            return Ok(STAKED_BALANCES_PRIMARY.get(store, &key));
        }

        // Find the closest block height
        let index = match user_staked_heights.binary_search(&height) {
            Ok(i) => i,                    // Exact match
            Err(i) => i.saturating_sub(1), // Closest lower height (prevent underflow)
        };

        // If we're querying for the most recent height, return primary
        if index == user_staked_heights.len() - 1 {
            Ok(STAKED_BALANCES_PRIMARY.get(store, &key))
        } else {
            // Return snapshot at the closest lower height
            let snapshot_height = user_staked_heights[index + 1];
            Ok(STAKED_BALANCES_SNAPSHOT.get(store, &(snapshot_height, key.clone())))
        }
    }
}
