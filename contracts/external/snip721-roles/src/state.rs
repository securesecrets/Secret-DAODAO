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
pub static MEMBERS_PRIMARY: Keymap<Addr, u64> = Keymap::new(b"staked_balances_primary");
pub static MEMBERS_SNAPSHOT: Keymap<(u64, Addr), u64> = Keymap::new(b"staked_balances_snapshot");
pub static MEMBERS_AT_HEIGHT: Keymap<Addr, Vec<u64>> = Keymap::new(b"user_Staked_at_height");

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
        let primary = MEMBERS_PRIMARY.get(store, &key.clone());
        if primary.is_none() {
            MEMBERS_PRIMARY.insert(store, &key.clone(), &value)?;
            MEMBERS_SNAPSHOT.insert(store, &(block_height, key.clone()), &default)?;
            MEMBERS_AT_HEIGHT.insert(store, &key.clone(), &vec![block_height])?;
        } else {
            let mut user_staked_height = MEMBERS_AT_HEIGHT.get(store, &key.clone()).unwrap();
            MEMBERS_SNAPSHOT.insert(store, &(block_height, key.clone()), &primary.unwrap())?;
            MEMBERS_PRIMARY.insert(store, &key.clone(), &value)?;
            user_staked_height.push(block_height);
            MEMBERS_AT_HEIGHT.insert(store, &key.clone(), &user_staked_height)?;
        }

        Ok(())
    }

    pub fn load(store: &dyn Storage, key: Addr) -> u64 {
        MEMBERS_PRIMARY.get(store, &key).unwrap_or_default()
    }

    pub fn may_load_at_height(
        store: &dyn Storage,
        key: Addr,
        height: u64,
    ) -> StdResult<Option<u64>> {
        let snapshot_key = (height, key.clone());

        let snapshot_value = MEMBERS_SNAPSHOT.get(store, &snapshot_key);
        if snapshot_value.is_none() {
            Ok(MEMBERS_PRIMARY.get(store, &key))
        } else {
            let x = MEMBERS_AT_HEIGHT.get(store, &key).unwrap();
            let id = match x.binary_search(&height) {
                Ok(index) => Some(index),
                Err(_) => None,
            };
            // return Ok(Some(Uint128::new(x.len() as u128)));
            if id.unwrap() == (x.len() - 1) {
                Ok(MEMBERS_PRIMARY.get(store, &key))
            } else {
                let snapshot_value =
                    MEMBERS_SNAPSHOT.get(store, &(x[id.unwrap() + 1_usize], key.clone()));
                Ok(snapshot_value)
            }
        }
    }
    pub fn remove(store: &mut dyn Storage, key: Addr) -> StdResult<()> {
        // Remove the member's data from all storage maps
        MEMBERS_PRIMARY.remove(store, &key)?;
        MEMBERS_AT_HEIGHT.remove(store, &key)?;

        // Remove all snapshot entries associated with the member
        let user_staked_height = MEMBERS_AT_HEIGHT.get(store, &key).unwrap_or_default();
        for height in user_staked_height {
            MEMBERS_SNAPSHOT.remove(store, &(height, key.clone()))?;
        }

        // Return Ok(()) if all removals were successful
        Ok(())
    }
}

/// A historic snapshot of total weight over time
pub const TOTAL_PRIMARY: Item<u64> = Item::new("staked_balances_primary");
pub static TOTAL_SNAPSHOT: Keymap<u64, u64> = Keymap::new(b"staked_balances_snapshot");
pub const TOTAL_AT_HEIGHTS: Item<Vec<u64>> = Item::new("user_Staked_at_height");

pub struct TotalStore {}
impl TotalStore {
    // Function to store a value at a specific block height
    pub fn save(store: &mut dyn Storage, block_height: u64, value: u64) -> StdResult<()> {
        let default: u64 = 0;
        let primary = TOTAL_PRIMARY.load(store).unwrap_or_default();
        if primary == 0 {
            TOTAL_PRIMARY.save(store, &value)?;
            TOTAL_SNAPSHOT.insert(store, &block_height, &default)?;
            TOTAL_AT_HEIGHTS.save(store, &vec![block_height])?;
        } else {
            let mut user_staked_height = TOTAL_AT_HEIGHTS.load(store).unwrap_or_default();
            TOTAL_SNAPSHOT.insert(store, &block_height, &primary)?;
            TOTAL_PRIMARY.save(store, &value)?;
            user_staked_height.push(block_height);
            TOTAL_AT_HEIGHTS.save(store, &user_staked_height)?;
        }

        Ok(())
    }

    pub fn load(store: &dyn Storage) -> u64 {
        TOTAL_PRIMARY.load(store).unwrap_or_default()
    }

    pub fn may_load_at_height(store: &dyn Storage, height: u64) -> StdResult<Option<u64>> {
        let snapshot_key = height;

        let snapshot_value = TOTAL_SNAPSHOT.get(store, &snapshot_key);
        if snapshot_value.is_none() {
            Ok(Some(TOTAL_PRIMARY.load(store).unwrap_or_default()))
        } else {
            let x = TOTAL_AT_HEIGHTS.load(store).unwrap_or_default();
            let id = match x.binary_search(&height) {
                Ok(index) => Some(index),
                Err(_) => None,
            };
            // return Ok(Some(Uint128::new(x.len() as u128)));
            if id.unwrap() == (x.len() - 1) {
                Ok(Some(TOTAL_PRIMARY.load(store).unwrap_or_default()))
            } else {
                let snapshot_value = TOTAL_SNAPSHOT.get(store, &(x[id.unwrap() + 1_usize]));
                Ok(snapshot_value)
            }
        }
    }
}
