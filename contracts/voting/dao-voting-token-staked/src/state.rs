use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, StdResult, Storage, Uint128};
use cw_hooks::Hooks;
use dao_interface::state::AnyContractInfo;
use dao_voting::threshold::ActiveThreshold;
use secret_cw_controllers::Claims;
use secret_storage_plus::Item;
use secret_toolkit::storage::Keymap;
use secret_utils::Duration;

use crate::msg::TokenInfo;

#[cw_serde]
pub struct Config {
    pub unstaking_duration: Option<Duration>,
}

/// The configuration of this voting contract
pub const CONFIG: Item<Config> = Item::new("config");

/// The address of the DAO this voting contract is connected to
pub const DAO: Item<AnyContractInfo> = Item::new("dao");

/// The native denom associated with this contract
pub const DENOM: Item<String> = Item::new("denom");

/// Keeps track of staked balances by address over time
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

/// Keeps track of staked total over time
pub const STAKED_TOTAL_PRIMARY: Item<Uint128> = Item::new("staked_balances_primary");
pub const STAKED_TOTAL_SNAPSHOT: Keymap<u64, Uint128> = Keymap::new(b"staked_balances_snapshot");
pub const STAKED_TOTAL_AT_HEIGHTS: Item<Vec<u64>> = Item::new("user_Staked_at_height");

pub struct TotalStakedStore {}
impl TotalStakedStore {
    // Function to store a value at a specific block height
    pub fn save(store: &mut dyn Storage, block_height: u64, value: Uint128) -> StdResult<()> {
        let primary = STAKED_TOTAL_PRIMARY.load(store).unwrap_or_default();
        if primary.is_zero() {
            STAKED_TOTAL_PRIMARY.save(store, &value)?;
            STAKED_TOTAL_SNAPSHOT.insert(store, &block_height, &Uint128::zero())?;
            STAKED_TOTAL_AT_HEIGHTS.save(store, &vec![block_height])?;
        } else {
            let mut user_staked_height = STAKED_TOTAL_AT_HEIGHTS.load(store).unwrap_or_default();
            STAKED_TOTAL_SNAPSHOT.insert(store, &block_height, &primary)?;
            STAKED_TOTAL_PRIMARY.save(store, &value)?;
            user_staked_height.push(block_height);
            STAKED_TOTAL_AT_HEIGHTS.save(store, &user_staked_height)?;
        }

        Ok(())
    }

    pub fn load(store: &dyn Storage) -> Uint128 {
        STAKED_TOTAL_PRIMARY.load(store).unwrap_or_default()
    }

    pub fn may_load_at_height(store: &dyn Storage, height: u64) -> StdResult<Option<Uint128>> {
        let snapshot_key = height;

        let snapshot_value = STAKED_TOTAL_SNAPSHOT.get(store, &snapshot_key);
        if snapshot_value.is_none() {
            Ok(Some(STAKED_TOTAL_PRIMARY.load(store).unwrap_or_default()))
        } else {
            let x = STAKED_TOTAL_AT_HEIGHTS.load(store).unwrap_or_default();
            let id = match x.binary_search(&height) {
                Ok(index) => Some(index),
                Err(_) => None,
            };
            // return Ok(Some(Uint128::new(x.len() as u128)));
            if id.unwrap() == (x.len() - 1) {
                return Ok(Some(STAKED_TOTAL_PRIMARY.load(store).unwrap_or_default()));
            } else {
                let snapshot_value =
                    STAKED_TOTAL_SNAPSHOT.get(store, &(x[id.unwrap() + 1 as usize]));
                return Ok(snapshot_value);
            }
        }
    }
}

/// The maximum number of claims that may be outstanding.
pub const MAX_CLAIMS: u64 = 100;

pub const CLAIMS: Claims = Claims::new("claims");

/// The minimum amount of staked tokens for the DAO to be active
pub const ACTIVE_THRESHOLD: Item<ActiveThreshold> = Item::new("active_threshold");

/// Hooks to contracts that will receive staking and unstaking messages
pub const HOOKS: Hooks = Hooks::new("hooks");

/// Temporarily holds token_instantiation_info when creating a new Token Factory denom
pub const TOKEN_INSTANTIATION_INFO: Item<TokenInfo> = Item::new("token_instantiation_info");

/// The address of the cw-tokenfactory-issuer contract
pub const TOKEN_ISSUER_CONTRACT: Item<Addr> = Item::new("token_issuer_contract");
