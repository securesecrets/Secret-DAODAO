use cosmwasm_std::{Addr, StdResult, Storage};
// use cw4::{
//     MEMBERS_CHANGELOG, MEMBERS_CHECKPOINTS, MEMBERS_KEY, TOTAL_KEY, TOTAL_KEY_CHANGELOG,
//     TOTAL_KEY_CHECKPOINTS,
// };
use secret_cw_controllers::{Admin, Hooks};
use secret_storage_plus::Item;
use secret_toolkit::storage::Keymap;
use shade_protocol::Contract;

pub const ADMIN: Admin = Admin::new("admin");
pub const HOOKS: Hooks = Hooks::new("cw4-hooks");
pub const QUERY_AUTH: Item<Contract> = Item::new("query_auth");

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

        // Load the current primary value, if it exists
        let primary = MEMBERS_PRIMARY.get(store, &key.clone());

        if primary.is_none() {
            // First time staking for this user
            MEMBERS_PRIMARY.insert(store, &key.clone(), &value)?;
            MEMBERS_SNAPSHOT.insert(store, &(block_height, key.clone()), &default)?;
            MEMBERS_AT_HEIGHT.insert(store, &key.clone(), &vec![block_height])?;
        } else {
            // Update staking info for an existing user
            let mut user_staked_height = MEMBERS_AT_HEIGHT
                .get(store, &key.clone())
                .unwrap_or_default();

            // Store the old primary value as a snapshot at the current block height
            MEMBERS_SNAPSHOT.insert(store, &(block_height, key.clone()), &primary.unwrap())?;
            MEMBERS_PRIMARY.insert(store, &key.clone(), &value)?;

            // Add the new block height to the user's list of heights if it's not a duplicate
            if !user_staked_height.contains(&block_height) {
                user_staked_height.push(block_height);
            }
            MEMBERS_AT_HEIGHT.insert(store, &key.clone(), &user_staked_height)?;
        }

        Ok(())
    }

    // Function to load the current staking balance of a user
    pub fn load(store: &dyn Storage, key: Addr) -> u64 {
        MEMBERS_PRIMARY.get(store, &key).unwrap_or_default()
    }

    // Function to load the staking balance of a user at a specific height
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

        // Otherwise, get the list of heights this user has staked at
        let user_staked_heights = MEMBERS_AT_HEIGHT.get(store, &key).unwrap_or_default();

        // If the user has never staked, return the current primary balance
        if user_staked_heights.is_empty() {
            return Ok(MEMBERS_PRIMARY.get(store, &key));
        }

        // Find the latest block height before or at the specified height
        let index = match user_staked_heights.binary_search(&height) {
            Ok(i) => i,                    // exact match found
            Err(i) => i.saturating_sub(1), // closest lower height, handle case where i = 0
        };

        // If the height is beyond the last checkpoint, return the primary balance
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
        let user_staked_height = MEMBERS_AT_HEIGHT.get(store, &key).unwrap_or_default();

        // Remove the primary and height data
        MEMBERS_PRIMARY.remove(store, &key)?;
        MEMBERS_AT_HEIGHT.remove(store, &key)?;

        // Remove all snapshot entries associated with the user
        for height in user_staked_height {
            MEMBERS_SNAPSHOT.remove(store, &(height, key.clone()))?;
        }

        Ok(())
    }
}

/// A historic snapshot of total weight over time
pub const TOTAL_PRIMARY: Item<u64> = Item::new("staked_balances_primary");
pub const TOTAL_SNAPSHOT: Keymap<u64, u64> = Keymap::new(b"staked_balances_snapshot");
pub const TOTAL_AT_HEIGHTS: Item<Vec<u64>> = Item::new("user_staked_at_height");

pub struct TotalStore {}

impl TotalStore {
    // Function to store a value at a specific block height
    pub fn save(store: &mut dyn Storage, block_height: u64, value: u64) -> StdResult<()> {
        let default: u64 = 0;

        // Load the current primary value, or default to 0 if not found
        let primary = TOTAL_PRIMARY.load(store).unwrap_or_default();

        if primary == 0 {
            // First time storing total weight
            TOTAL_PRIMARY.save(store, &value)?;
            TOTAL_SNAPSHOT.insert(store, &block_height, &default)?;
            TOTAL_AT_HEIGHTS.save(store, &vec![block_height])?;
        } else {
            // Update total weight for existing entries
            let mut staked_heights = TOTAL_AT_HEIGHTS.load(store).unwrap_or_default();

            // Insert the old primary value into the snapshot at the current block height
            TOTAL_SNAPSHOT.insert(store, &block_height, &primary)?;
            TOTAL_PRIMARY.save(store, &value)?;

            // Add the new block height if it's not a duplicate
            if !staked_heights.contains(&block_height) {
                staked_heights.push(block_height);
            }
            TOTAL_AT_HEIGHTS.save(store, &staked_heights)?;
        }

        Ok(())
    }

    // Function to load the current total staked value
    pub fn load(store: &dyn Storage) -> u64 {
        TOTAL_PRIMARY.load(store).unwrap_or_default()
    }

    // Function to load the total staked value at a specific block height
    pub fn may_load_at_height(store: &dyn Storage, height: u64) -> StdResult<Option<u64>> {
        let snapshot_value = TOTAL_SNAPSHOT.get(store, &height);

        // If a snapshot exists at the exact height, return it
        if snapshot_value.is_some() {
            return Ok(snapshot_value);
        }

        // Otherwise, load all the heights where total staked was recorded
        let staked_heights = TOTAL_AT_HEIGHTS.load(store).unwrap_or_default();

        // If there are no staked heights, return the current total primary value
        if staked_heights.is_empty() {
            return Ok(Some(TOTAL_PRIMARY.load(store).unwrap_or_default()));
        }

        // Find the latest block height before or at the specified height
        let index = match staked_heights.binary_search(&height) {
            Ok(i) => i,                    // Exact match found
            Err(i) => i.saturating_sub(1), // Closest lower height, handle case where i = 0
        };

        // If the height is beyond the last checkpoint, return the current primary value
        if index == staked_heights.len() - 1 {
            Ok(Some(TOTAL_PRIMARY.load(store).unwrap_or_default()))
        } else {
            // Otherwise, return the snapshot at the closest lower height
            let snapshot_height = staked_heights[index];
            Ok(TOTAL_SNAPSHOT.get(store, &snapshot_height))
        }
    }
}
