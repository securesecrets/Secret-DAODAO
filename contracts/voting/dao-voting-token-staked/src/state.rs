use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, StdResult, Storage, Uint128};
use cw_hooks::Hooks;
use dao_interface::state::AnyContractInfo;
use dao_voting::threshold::ActiveThreshold;
use secret_cw_controllers::Claims;
use secret_storage_plus::Item;
use secret_toolkit::storage::Keymap;
use secret_utils::Duration;
use shade_protocol::Contract;

use crate::msg::TokenInfo;

#[cw_serde]
pub struct Config {
    pub unstaking_duration: Option<Duration>,
    pub query_auth: Contract,
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
        // Get the current primary balance for the address
        let primary_balance = STAKED_BALANCES_PRIMARY.get(store, &key).unwrap_or_default();

        // If there's no primary balance, initialize it
        if primary_balance.is_zero() {
            STAKED_BALANCES_PRIMARY.insert(store, &key, &value)?;
            STAKED_BALANCES_SNAPSHOT.insert(
                store,
                &(block_height, key.clone()),
                &Uint128::zero(),
            )?;
            USER_STAKED_AT_HEIGHT.insert(store, &key, &vec![block_height])?;
        } else {
            // Update the existing balance
            STAKED_BALANCES_SNAPSHOT.insert(
                store,
                &(block_height, key.clone()),
                &primary_balance,
            )?;
            STAKED_BALANCES_PRIMARY.insert(store, &key, &value)?;

            // Update the list of staked heights for the user
            let mut user_staked_height = USER_STAKED_AT_HEIGHT.get(store, &key).unwrap();
            user_staked_height.push(block_height);
            USER_STAKED_AT_HEIGHT.insert(store, &key, &user_staked_height)?;
        }

        Ok(())
    }

    // Load the current staked balance for an address
    pub fn load(store: &dyn Storage, key: Addr) -> Uint128 {
        STAKED_BALANCES_PRIMARY.get(store, &key).unwrap_or_default()
    }

    // Load the staked balance at a specific block height, falling back to the current balance if not found
    pub fn may_load_at_height(
        store: &dyn Storage,
        key: Addr,
        height: u64,
    ) -> StdResult<Option<Uint128>> {
        // Check for a snapshot at the given height
        let snapshot_key = (height, key.clone());
        if let Some(snapshot_value) = STAKED_BALANCES_SNAPSHOT.get(store, &snapshot_key) {
            return Ok(Some(snapshot_value));
        }

        // If no snapshot exists, check the primary balance
        let primary_balance = STAKED_BALANCES_PRIMARY.get(store, &key).unwrap_or_default();
        let staked_heights = USER_STAKED_AT_HEIGHT.get(store, &key).unwrap_or_default();

        // Check the index of the height in the staked heights
        match staked_heights.binary_search(&height) {
            Ok(index) if index == staked_heights.len() - 1 => Ok(Some(primary_balance)), // Last index
            Ok(index) => {
                let next_height = staked_heights[index + 1];
                Ok(STAKED_BALANCES_SNAPSHOT.get(store, &(next_height, key)))
            }
            Err(_) => Ok(Some(primary_balance)), // Height not found, return current balance
        }
    }
}

/// Keeps track of staked total over time
pub const STAKED_TOTAL_PRIMARY: Item<Uint128> = Item::new("staked_balances_primary");
pub static STAKED_TOTAL_SNAPSHOT: Keymap<u64, Uint128> = Keymap::new(b"staked_balances_snapshot");
pub const STAKED_TOTAL_AT_HEIGHTS: Item<Vec<u64>> = Item::new("user_staked_at_height");

pub struct TotalStakedStore {}

impl TotalStakedStore {
    // Function to store a value at a specific block height
    pub fn save(store: &mut dyn Storage, block_height: u64, value: Uint128) -> StdResult<()> {
        // Load the current total staked amount
        let primary_total = STAKED_TOTAL_PRIMARY.load(store).unwrap_or_default();

        // If there is no staked total, initialize it
        if primary_total.is_zero() {
            STAKED_TOTAL_PRIMARY.save(store, &value)?;
            STAKED_TOTAL_SNAPSHOT.insert(store, &block_height, &Uint128::zero())?;
            STAKED_TOTAL_AT_HEIGHTS.save(store, &vec![block_height])?;
        } else {
            // Update existing totals
            let mut user_staked_heights = STAKED_TOTAL_AT_HEIGHTS.load(store).unwrap_or_default();
            STAKED_TOTAL_SNAPSHOT.insert(store, &block_height, &primary_total)?;
            STAKED_TOTAL_PRIMARY.save(store, &value)?;
            user_staked_heights.push(block_height);
            STAKED_TOTAL_AT_HEIGHTS.save(store, &user_staked_heights)?;
        }

        Ok(())
    }

    // Load the current total staked amount
    pub fn load(store: &dyn Storage) -> Uint128 {
        STAKED_TOTAL_PRIMARY.load(store).unwrap_or_default()
    }

    // Load the staked total at a specific block height, falling back to the current total if not found
    pub fn may_load_at_height(store: &dyn Storage, height: u64) -> StdResult<Option<Uint128>> {
        // Check for a snapshot at the given height
        let snapshot_value = STAKED_TOTAL_SNAPSHOT.get(store, &height);

        // If a snapshot exists, return it
        if let Some(value) = snapshot_value {
            return Ok(Some(value));
        }

        // If no snapshot exists, check the primary total
        let primary_total = STAKED_TOTAL_PRIMARY.load(store).unwrap_or_default();
        let staked_heights = STAKED_TOTAL_AT_HEIGHTS.load(store).unwrap_or_default();

        // Check the index of the height in the staked heights
        match staked_heights.binary_search(&height) {
            Ok(index) if index == staked_heights.len() - 1 => Ok(Some(primary_total)), // Last index
            Ok(index) => {
                let next_height = staked_heights[index + 1];
                Ok(STAKED_TOTAL_SNAPSHOT.get(store, &next_height))
            }
            Err(_) => Ok(Some(primary_total)), // Height not found, return current total
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
