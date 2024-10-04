use cosmwasm_std::{Addr, StdResult, Storage};
use schemars::JsonSchema;
use secret_cw_controllers::Hooks;
use secret_storage_plus::Item;
use secret_toolkit::storage::Keymap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema, Debug, Default)]
pub struct Config {
    pub contract_address: String,
    pub code_hash: String,
}

// Hooks to contracts that will receive staking and unstaking messages.
pub const HOOKS: Hooks = Hooks::new("hooks");
pub const SNIP721_INFO: Item<Config> = Item::new("si");

// /// A historic snapshot of total weight over time
// pub const TOTAL: SnapshotItem<u64> = SnapshotItem::new(
//     "total",
//     "total__checkpoints",
//     "total__changelog",
//     Strategy::EveryBlock,
// );

// /// A historic list of members and total voting weights
// pub const MEMBERS: SnapshotMap<&Addr, u64> = SnapshotMap::new(
//     "members",
//     "members__checkpoints",
//     "members__changelog",
//     Strategy::EveryBlock,
// );

/// A historic list of members and total voting weights
pub const MEMBERS_PRIMARY: Keymap<Addr, u64> = Keymap::new(b"staked_balances_primary");
pub const MEMBERS_SNAPSHOT: Keymap<(u64, Addr), u64> = Keymap::new(b"staked_balances_snapshot");
pub const MEMBERS_AT_HEIGHT: Keymap<Addr, Vec<u64>> = Keymap::new(b"user_staked_at_height");

pub struct MembersStore {}

impl MembersStore {
    // Function to store a value at a specific block height
    pub fn save(
        store: &mut dyn Storage,
        block_height: u64,
        key: Addr,
        value: u64,
    ) -> StdResult<()> {
        let default: u64 = 0;

        // Load the current primary value
        let primary = MEMBERS_PRIMARY.get(store, &key);

        if primary.is_none() {
            // First time staking for this user
            MEMBERS_PRIMARY.insert(store, &key, &value)?;
            MEMBERS_SNAPSHOT.insert(store, &(block_height, key.clone()), &default)?;
            MEMBERS_AT_HEIGHT.insert(store, &key, &vec![block_height])?;
        } else {
            // Update staking info for an existing user
            let mut user_staked_height = MEMBERS_AT_HEIGHT.get(store, &key).unwrap_or_default();

            // Insert the old primary value as a snapshot at the given block height
            MEMBERS_SNAPSHOT.insert(store, &(block_height, key.clone()), &primary.unwrap())?;
            MEMBERS_PRIMARY.insert(store, &key, &value)?;

            // Add the block height if it's not already present
            if !user_staked_height.contains(&block_height) {
                user_staked_height.push(block_height);
            }

            MEMBERS_AT_HEIGHT.insert(store, &key, &user_staked_height)?;
        }

        Ok(())
    }

    // Function to load the current staking balance of a user
    pub fn load(store: &dyn Storage, key: Addr) -> u64 {
        MEMBERS_PRIMARY.get(store, &key).unwrap_or_default()
    }

    // Function to load the staking balance of a user at a specific block height
    pub fn may_load_at_height(
        store: &dyn Storage,
        key: Addr,
        height: u64,
    ) -> StdResult<Option<u64>> {
        let snapshot_key = (height, key.clone());
        let snapshot_value = MEMBERS_SNAPSHOT.get(store, &snapshot_key);

        // If there's a snapshot at the exact height, return it
        if snapshot_value.is_some() {
            return Ok(snapshot_value);
        }

        // Load all the heights where the user has staked
        let user_staked_heights = MEMBERS_AT_HEIGHT.get(store, &key).unwrap_or_default();

        // If there are no staked heights, return the current primary balance
        if user_staked_heights.is_empty() {
            return Ok(MEMBERS_PRIMARY.get(store, &key));
        }

        // Find the latest block height before or at the given height
        let index = match user_staked_heights.binary_search(&height) {
            Ok(i) => i,                    // Exact match
            Err(i) => i.saturating_sub(1), // Closest lower height, safe from underflow
        };

        // If the height is beyond the last checkpoint, return the current primary balance
        if index == user_staked_heights.len() - 1 {
            Ok(MEMBERS_PRIMARY.get(store, &key))
        } else {
            // Otherwise, return the snapshot at the closest lower height
            let snapshot_height = user_staked_heights[index];
            Ok(MEMBERS_SNAPSHOT.get(store, &(snapshot_height, key)))
        }
    }

    // Function to remove a user's staking data
    pub fn remove(store: &mut dyn Storage, key: Addr) -> StdResult<()> {
        // Get the user's staked heights before removing their data
        let user_staked_heights = MEMBERS_AT_HEIGHT.get(store, &key).unwrap_or_default();

        // Remove the primary and height data
        MEMBERS_PRIMARY.remove(store, &key)?;
        MEMBERS_AT_HEIGHT.remove(store, &key)?;

        // Remove all snapshot entries associated with the user
        for height in user_staked_heights {
            MEMBERS_SNAPSHOT.remove(store, &(height, key.clone()))?;
        }

        Ok(())
    }
}

/// A historic snapshot of total weight over time
pub const TOTAL_PRIMARY: Item<u64> = Item::new("staked_balances_primary");
pub const TOTAL_SNAPSHOT: Keymap<u64, u64> = Keymap::new(b"staked_balances_snapshot");
pub const TOTAL_AT_HEIGHTS: Item<Vec<u64>> = Item::new("total_staked_at_height");

pub struct TotalStore {}

impl TotalStore {
    // Function to store a value at a specific block height
    pub fn save(store: &mut dyn Storage, block_height: u64, value: u64) -> StdResult<()> {
        let default: u64 = 0;
        let primary = TOTAL_PRIMARY.load(store).unwrap_or_default();

        if primary == 0 {
            // First time total weight is stored
            TOTAL_PRIMARY.save(store, &value)?;
            TOTAL_SNAPSHOT.insert(store, &block_height, &default)?;

            TOTAL_AT_HEIGHTS.save(store, &vec![block_height])?;
        } else {
            // Update existing total weight
            let mut total_staked_height = TOTAL_AT_HEIGHTS.load(store).unwrap_or_default();

            // Insert the old primary value as a snapshot at the given block height
            TOTAL_SNAPSHOT.insert(store, &block_height, &primary)?;

            // Update primary with the new total weight
            TOTAL_PRIMARY.save(store, &value)?;

            // Ensure no duplicate heights are inserted
            if !total_staked_height.contains(&block_height) {
                total_staked_height.push(block_height);
            }

            TOTAL_AT_HEIGHTS.save(store, &total_staked_height)?;
        }

        Ok(())
    }

    // Function to load the current total weight
    pub fn load(store: &dyn Storage) -> u64 {
        TOTAL_PRIMARY.load(store).unwrap_or_default()
    }

    // Function to load the total weight at a specific block height
    pub fn may_load_at_height(store: &dyn Storage, height: u64) -> StdResult<Option<u64>> {
        // Try to fetch a snapshot at the exact height
        let snapshot_value = TOTAL_SNAPSHOT.get(store, &height);

        // If there's no snapshot at the exact height, return the current primary value
        if snapshot_value.is_none() {
            return Ok(Some(TOTAL_PRIMARY.load(store).unwrap_or_default()));
        }

        // Fetch the block heights where total weight was updated
        let total_staked_heights = TOTAL_AT_HEIGHTS.load(store).unwrap_or_default();

        // If no heights exist, return the primary value
        if total_staked_heights.is_empty() {
            return Ok(Some(TOTAL_PRIMARY.load(store).unwrap_or_default()));
        }

        // Find the closest block height (binary search)
        let index = match total_staked_heights.binary_search(&height) {
            Ok(i) => i,                    // Exact match
            Err(i) => i.saturating_sub(1), // Closest lower height (safe from underflow)
        };

        // If the given height is beyond the last checkpoint, return the current primary value
        if index == total_staked_heights.len() - 1 {
            Ok(Some(TOTAL_PRIMARY.load(store).unwrap_or_default()))
        } else {
            // Return the snapshot at the closest lower height
            let snapshot_height = total_staked_heights[index];
            Ok(TOTAL_SNAPSHOT.get(store, &snapshot_height))
        }
    }
}
