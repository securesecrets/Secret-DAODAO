use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary, Empty, StdError, StdResult, Storage, Uint128};
use cw_hooks::Hooks;
use dao_interface::state::AnyContractInfo;
use dao_voting::threshold::ActiveThreshold;
use secret_storage_plus::Item;
use secret_toolkit::storage::Keymap;
use secret_utils::Duration;
use snip721_controllers::NftClaims;

use crate::error::ContractError;

#[cw_serde]
pub struct Config {
    pub nft_address: Addr,
    pub nft_code_hash: String,
    pub unstaking_duration: Option<Duration>,
}

pub const ACTIVE_THRESHOLD: Item<ActiveThreshold> = Item::new("active_threshold");
pub const CONFIG: Item<Config> = Item::new("config");
pub const DAO: Item<AnyContractInfo> = Item::new("dao");

// Holds initial NFTs messages during instantiation.
pub const INITIAL_NFTS: Item<Vec<Binary>> = Item::new("initial_nfts");

/// The set of NFTs currently staked by each address. The existence of
/// an `(address, token_id)` pair implies that `address` has staked
/// `token_id`.
pub static STAKED_NFTS_PER_OWNER: Keymap<(Addr, String), Empty> = Keymap::new(b"snpw");

/// The number of NFTs staked by an address as a function of block
/// height.
pub static NFT_BALANCES_PRIMARY: Keymap<Addr, Uint128> = Keymap::new(b"nft_balances_primary");
pub static NFT_BALANCES_SNAPSHOT: Keymap<(u64, Addr), Uint128> =
    Keymap::new(b"nft_balances_snapshot");
pub static USER_STAKED_NFT_AT_HEIGHT: Keymap<Addr, Vec<u64>> =
    Keymap::new(b"user_Staked_Nft_at_height");

pub struct NftBalancesStore {}
impl NftBalancesStore {
    // Function to store a value at a specific block height
    pub fn save(
        store: &mut dyn Storage,
        block_height: u64,
        key: Addr,
        value: Uint128,
    ) -> StdResult<()> {
        let primary = NFT_BALANCES_PRIMARY.get(store, &key.clone());
        if primary.is_none() {
            NFT_BALANCES_PRIMARY.insert(store, &key.clone(), &value)?;
            NFT_BALANCES_SNAPSHOT.insert(store, &(block_height, key.clone()), &Uint128::zero())?;
            USER_STAKED_NFT_AT_HEIGHT.insert(store, &key.clone(), &vec![block_height])?;
        } else {
            let mut user_staked_height =
                USER_STAKED_NFT_AT_HEIGHT.get(store, &key.clone()).unwrap();
            NFT_BALANCES_SNAPSHOT.insert(store, &(block_height, key.clone()), &primary.unwrap())?;
            NFT_BALANCES_PRIMARY.insert(store, &key.clone(), &value)?;
            user_staked_height.push(block_height);
            USER_STAKED_NFT_AT_HEIGHT.insert(store, &key.clone(), &user_staked_height)?;
        }

        Ok(())
    }

    pub fn load(store: &dyn Storage, key: Addr) -> Uint128 {
        NFT_BALANCES_PRIMARY.get(store, &key).unwrap_or_default()
    }

    pub fn may_load_at_height(
        store: &dyn Storage,
        key: Addr,
        height: u64,
    ) -> StdResult<Option<Uint128>> {
        let snapshot_key = (height, key.clone());

        let snapshot_value = NFT_BALANCES_SNAPSHOT.get(store, &snapshot_key);
        if snapshot_value.is_none() {
            Ok(NFT_BALANCES_PRIMARY.get(store, &key))
        } else {
            let x = USER_STAKED_NFT_AT_HEIGHT.get(store, &key).unwrap();
            let id = match x.binary_search(&height) {
                Ok(index) => Some(index),
                Err(_) => None,
            };
            // return Ok(Some(Uint128::new(x.len() as u128)));
            if id.unwrap() == (x.len() - 1) {
                Ok(NFT_BALANCES_PRIMARY.get(store, &key))
            } else {
                let snapshot_value =
                    NFT_BALANCES_SNAPSHOT.get(store, &(x[id.unwrap() + 1_usize], key.clone()));
                Ok(snapshot_value)
            }
        }
    }
}

/// The number of NFTs staked with this contract as a function of
/// block height.
pub const TOTAL_STAKED_NFTS_PRIMARY: Item<Uint128> = Item::new("tsnP");
pub static TOTAL_STAKED_NFTS_SNAPSHOT: Keymap<u64, Uint128> = Keymap::new(b"tsns");
pub const TOTAL_STAKED_NFTS_AT_HEIGHTS: Item<Vec<u64>> = Item::new("tsnah");

pub struct StakedNftsTotalStore {}
impl StakedNftsTotalStore {
    // Function to store a value at a specific block height
    pub fn save(store: &mut dyn Storage, block_height: u64, value: Uint128) -> StdResult<()> {
        let primary = TOTAL_STAKED_NFTS_PRIMARY.load(store).unwrap_or_default();
        if primary.is_zero() {
            TOTAL_STAKED_NFTS_PRIMARY.save(store, &value)?;
            TOTAL_STAKED_NFTS_SNAPSHOT.insert(store, &block_height, &Uint128::zero())?;
            TOTAL_STAKED_NFTS_AT_HEIGHTS.save(store, &vec![block_height])?;
        } else {
            let mut user_staked_height = TOTAL_STAKED_NFTS_AT_HEIGHTS.load(store)?;
            TOTAL_STAKED_NFTS_SNAPSHOT.insert(store, &block_height, &primary)?;
            TOTAL_STAKED_NFTS_PRIMARY.save(store, &value)?;
            user_staked_height.push(block_height);
            TOTAL_STAKED_NFTS_AT_HEIGHTS.save(store, &user_staked_height)?;
        }

        Ok(())
    }

    pub fn load(store: &dyn Storage) -> Uint128 {
        TOTAL_STAKED_NFTS_PRIMARY.load(store).unwrap_or_default()
    }

    pub fn may_load_at_height(store: &dyn Storage, height: u64) -> StdResult<Option<Uint128>> {
        let snapshot_key = height;

        let snapshot_value = TOTAL_STAKED_NFTS_SNAPSHOT.get(store, &snapshot_key);
        if snapshot_value.is_none() {
            Ok(Some(TOTAL_STAKED_NFTS_PRIMARY.load(store)?))
        } else {
            let x = TOTAL_STAKED_NFTS_AT_HEIGHTS.load(store).unwrap();
            let id = match x.binary_search(&height) {
                Ok(index) => Some(index),
                Err(_) => None,
            };
            // return Ok(Some(Uint128::new(x.len() as u128)));
            if id.unwrap() == (x.len() - 1) {
                Ok(Some(TOTAL_STAKED_NFTS_PRIMARY.load(store)?))
            } else {
                let snapshot_value =
                    TOTAL_STAKED_NFTS_SNAPSHOT.get(store, &(x[id.unwrap() + 1_usize]));
                Ok(snapshot_value)
            }
        }
    }
}

/// The maximum number of claims that may be outstanding.
pub const MAX_CLAIMS: u64 = 70;
pub static NFT_CLAIMS: NftClaims = NftClaims::new(b"nft_claims");

// Hooks to contracts that will receive staking and unstaking
// messages.
pub const HOOKS: Hooks = Hooks::new("hooks");

pub fn register_staked_nft(
    storage: &mut dyn Storage,
    height: u64,
    staker: Addr,
    token_id: String,
) -> StdResult<()> {
    // let add_one = |prev: Option<Uint128>| -> StdResult<Uint128> {
    //     prev.unwrap_or_default()
    //         .checked_add(Uint128::new(1))
    //         .map_err(StdError::overflow)
    // };

    STAKED_NFTS_PER_OWNER.insert(storage, &(staker.clone(), token_id), &Empty::default())?;
    let value = NftBalancesStore::may_load_at_height(storage, staker.clone(), height)?;
    NftBalancesStore::save(
        storage,
        height,
        staker,
        value
            .unwrap()
            .checked_add(Uint128::new(1))
            .map_err(StdError::overflow)?,
    )?;

    let res = StakedNftsTotalStore::may_load_at_height(storage, height)?;
    StakedNftsTotalStore::save(
        storage,
        height,
        res.unwrap()
            .checked_add(Uint128::new(1))
            .map_err(StdError::overflow)?,
    )?;
    Ok(())
}

/// Registers the unstaking of TOKEN_IDs in storage. Errors if:
///
/// 1. `token_ids` is non-unique.
/// 2. a NFT being staked has not previously been staked.
pub fn register_unstaked_nfts(
    storage: &mut dyn Storage,
    height: u64,
    staker: Addr,
    token_ids: &[String],
) -> Result<(), ContractError> {
    // let subtractor = |amount: u128| {
    //     move |prev: Option<Uint128>| -> StdResult<Uint128> {
    //         prev.expect("unstaking that which was not staked")
    //             .checked_sub(Uint128::new(amount))
    //             .map_err(StdError::overflow)
    //     }
    // };

    for token in token_ids {
        let key = &(staker.clone(), token.clone());
        if STAKED_NFTS_PER_OWNER.contains(storage, key) {
            let _ = STAKED_NFTS_PER_OWNER.remove(storage, key);
        } else {
            return Err(ContractError::NotStaked {
                token_id: token.clone(),
            });
        }
    }

    // invariant: token_ids has unique values. for loop asserts this.

    // let sub_n = subtractor(token_ids.len() as u128);
    // TOTAL_STAKED_NFTS.update(storage, height, sub_n)?;
    // NFT_BALANCES.update(storage, staker, height, sub_n)?;

    let value = NftBalancesStore::may_load_at_height(storage, staker.clone(), height)?;
    NftBalancesStore::save(
        storage,
        height,
        staker,
        value
            .unwrap()
            .checked_sub(Uint128::new(token_ids.len() as u128))
            .map_err(StdError::overflow)?,
    )?;

    let res = StakedNftsTotalStore::may_load_at_height(storage, height)?;
    StakedNftsTotalStore::save(
        storage,
        height,
        res.unwrap()
            .checked_sub(Uint128::new(token_ids.len() as u128))
            .map_err(StdError::overflow)?,
    )?;
    Ok(())
}
