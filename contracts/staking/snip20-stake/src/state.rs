use cosmwasm_std::{Addr, StdResult, Storage, Uint128};
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

pub const STAKED_TOTAL_AT_HEIGHT: Keymap<u64, Uint128> = Keymap::new(b"staked_total");

pub struct StakedTotalStore {}
impl StakedTotalStore {
    // Function to store a value at a specific block height
    pub fn save(store: &mut dyn Storage, block_height: u64, value: Uint128) -> StdResult<()> {
        STAKED_TOTAL_AT_HEIGHT.insert(store, &block_height, &value)?;
        Ok(())
    }

    pub fn load(store: &dyn Storage) -> Uint128 {
        BALANCE.load(store).unwrap_or_default()
    }

    pub fn may_load_at_height(store: &dyn Storage, height: u64) -> StdResult<Option<Uint128>> {
        let total_staked_at_height = STAKED_TOTAL_AT_HEIGHT.get(store, &height);
        if total_staked_at_height.is_none() {
            let res = BALANCE.load(store)?;
            return Ok(Some(res));
        } else {
            let snapshot_value = STAKED_TOTAL_AT_HEIGHT.get(store, &height);
            return Ok(snapshot_value);
        }
    }
}

pub const STAKED_BALANCES_PRIMARY: Keymap<Addr, Uint128> = Keymap::new(b"staked_balances_primary");
pub const STAKED_BALANCES_SNAPSHOT: Keymap<(u64, Addr), Uint128> =
    Keymap::new(b"staked_balances_snapshot");
pub const USER_STAKED_AT_HEIGHT: Keymap<Addr, Vec<u64>> = Keymap::new(b"user_Staked_at_height");

pub struct StakedBalancesStore {}
impl StakedBalancesStore {
    // Function to store a value at a specific block height
    pub fn save(
        store: &mut dyn Storage,
        block_height: u64,
        key: Addr,
        value: Uint128,
    ) -> StdResult<()> {
        let primary = STAKED_BALANCES_PRIMARY.get(store, &key.clone());
        if primary.is_none() {
            STAKED_BALANCES_PRIMARY.insert(store, &key.clone(), &value)?;
            STAKED_BALANCES_SNAPSHOT.insert(
                store,
                &(block_height, key.clone()),
                &Uint128::zero(),
            )?;
            USER_STAKED_AT_HEIGHT.insert(store, &key.clone(), &vec![block_height])?;
        } else {
            let mut user_staked_height = USER_STAKED_AT_HEIGHT.get(store, &key.clone()).unwrap();
            STAKED_BALANCES_SNAPSHOT.insert(
                store,
                &(block_height, key.clone()),
                &primary.unwrap(),
            )?;
            STAKED_BALANCES_PRIMARY.insert(store, &key.clone(), &value)?;
            user_staked_height.push(block_height);
            USER_STAKED_AT_HEIGHT.insert(store, &key.clone(), &user_staked_height)?;
        }

        Ok(())
    }

    pub fn load(store: &dyn Storage, key: Addr) -> Uint128 {
        STAKED_BALANCES_PRIMARY.get(store, &key).unwrap_or_default()
    }

    pub fn may_load_at_height(
        store: &dyn Storage,
        key: Addr,
        height: u64,
    ) -> StdResult<Option<Uint128>> {
        let snapshot_key = (height, key.clone());

        let snapshot_value = STAKED_BALANCES_SNAPSHOT.get(store, &snapshot_key);
        if snapshot_value.is_none() {
            Ok(STAKED_BALANCES_PRIMARY.get(store, &key))
        } else {
            let x = USER_STAKED_AT_HEIGHT.get(store, &key).unwrap();
            let id = match x.binary_search(&height) {
                Ok(index) => Some(index),
                Err(_) => None,
            };
            // return Ok(Some(Uint128::new(x.len() as u128)));
            if id.unwrap() == (x.len() - 1) {
                return Ok(STAKED_BALANCES_PRIMARY.get(store, &key));
            } else {
                let snapshot_value = STAKED_BALANCES_SNAPSHOT
                    .get(store, &(x[id.unwrap() + 1 as usize], key.clone()));
                return Ok(snapshot_value);
            }
        }
    }
}
