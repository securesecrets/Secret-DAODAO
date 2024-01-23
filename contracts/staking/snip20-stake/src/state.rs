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

pub const STAKED_TOTAL_PRIMARY: Item<Uint128> =  Item::new("staked_total_primary");
pub const STAKED_TOTAL_SNAPSHOT: Keymap<u64,Uint128> =  Keymap::new(b"staked_total_primary");
pub const STAKED_TOTAL_VALUES: Keymap<u64,Uint128> =  Keymap::new(b"staked_total_values");


pub struct StakedTotalStore {}
impl StakedTotalStore {
    // Function to store a value at a specific block height
    pub fn save(
        store: &mut dyn Storage,
        block_height: u64,
        value: Uint128,
    )  -> StdResult<()>{
        // Save the old value to snapshots
        let snapshotvalue = STAKED_TOTAL_VALUES.get(store, &(block_height-1)).unwrap_or_default();
        let _ = STAKED_TOTAL_SNAPSHOT.insert(store, &block_height, &snapshotvalue);

        // Save the new value to primary
        let _ = STAKED_TOTAL_PRIMARY.save(store, &value);

        // Save the new value to values
       STAKED_TOTAL_VALUES.insert(store, &block_height, &value)?;

        Ok(())

     
    }

    pub fn load(
        store: &dyn Storage,
    ) -> Uint128 {
            STAKED_TOTAL_PRIMARY.load(store).unwrap_or_default()
    }

        pub fn may_load_at_height(
            store: &dyn Storage,
            height: u64
        ) -> StdResult<Option<Uint128>>  {

             let snapshot_value = STAKED_TOTAL_SNAPSHOT.get(store,&height);
             if let Some(r) = snapshot_value {
                 return Ok(Some(r));
             }
             else {
                return Ok(Some(STAKED_TOTAL_PRIMARY.load(store)?))

             }

             
    }
}

pub const   STAKED_BALANCES_PRIMARY: Keymap<Addr,Uint128> = Keymap::new(b"staked_balances_primary");
pub const   STAKED_BALANCES_SNAPSHOT: Keymap<(u64,Addr),Uint128> = Keymap::new(b"staked_balances_snapshot");
pub const   STAKED_BALANCES_VALUES: Keymap<(u64,Addr),Uint128> = Keymap::new(b"staked_balances_values");



pub struct StakedBalancesStore {}
impl StakedBalancesStore {
    // Function to store a value at a specific block height
    pub fn save(
        store: &mut dyn Storage,
        block_height: u64,
        key: Addr,
        value: Uint128,
    ) -> StdResult<()> {
         // Save the old value to snapshots
         let snapshotvalue = STAKED_BALANCES_VALUES.get(store, &(block_height-1,key.clone())).unwrap_or_default();
         let _ = STAKED_BALANCES_SNAPSHOT.insert(store, &(block_height, key.clone()), &snapshotvalue);
 
         // Save the new value to primary
         let _ = STAKED_BALANCES_PRIMARY.insert(store, &key.clone(), &value);

         // Save the new value to values
        STAKED_BALANCES_VALUES.insert(store, &(block_height, key), &value)?;
 
         Ok(())
    }

    pub fn load(
        store: &dyn Storage,
        key: Addr,
    ) -> Uint128 {
            STAKED_BALANCES_PRIMARY.get(store, &key).unwrap_or_default()
    }

        pub fn may_load_at_height(
            store: &dyn Storage,
            key: Addr,
            height: u64
        ) -> StdResult<Option<Uint128>>  {
            let snapshot_key = (height, key.clone());

             let snapshot_value = STAKED_BALANCES_SNAPSHOT.get(store,&snapshot_key);
             if let Some(r) = snapshot_value {
                 return Ok(Some(r));
             }
             else {
                return Ok(STAKED_BALANCES_PRIMARY.get(store, &key))

             }
        
    }
}

// pub struct SnapshotMap<'a> {
//     primary: Keymap<'a, (u64, Addr), Uint128>,
//     snapshots: Keymap<'a, (u64, Addr), Uint128>,

// }

// impl<'a> SnapshotMap<'a> {
//     pub const fn new() -> Self {
//         let primary = Keymap::new(b"primary");
//         let snapshots = Keymap::new(b"snapshots");
//         Self { primary, snapshots }
//     }

//     pub fn save(
//         &mut self,
//         store: &mut dyn Storage,
//         height: u64,
//         key: Addr,
//         value: Uint128,
//     ) -> StdResult<()> {
//         // Save the old value to snapshots
//         let snapshotvalue = self.primary.get(store, &(height-1,key.clone()));
//         let _ = self.snapshots.insert(store, &(height, key.clone()), &snapshotvalue.unwrap());

//         // Save the new value to primary
//         let _ = self.primary.insert(store, &(height,key.clone()), &value);

//         Ok(())
//     }

//     pub fn load(&self, store: &dyn Storage, height: Option<u64>, key: Addr) -> Uint128 {
//         let snapshot_key = (height.unwrap(), key);

//         // Check if there is a snapshotted value at the provided height
//          let snapshot_value = self.snapshots.get(store, &snapshot_key).unwrap();

//          if Some(snapshot_value).is_some(){
//             snapshot_value
//          }
//          else {
//             // If not, return the current value from primary or zero if key is not found.
//             self.primary.get(store, &snapshot_key).unwrap_or_default()
//         }
             
        
//     }

//     pub fn list(&self,store: &dyn Storage,start: u32,end: u32) ->StdResult<Vec<((u64,Addr), Uint128)>> {
//         let items = KeyItemIter::new(&self.primary, store, start, end)
//         .flatten()
//         .collect();

//     Ok(items)
//     }

//     pub fn get_len(&self,store: &dyn Storage) -> u32 {
//         self.primary.get_len(store).unwrap()
//     }
   
// }



// pub struct SnapshotItem<'a> {
//     primary: Keymap<'a, u64, Uint128>,
//     snapshots: Keymap<'a, u64, Uint128>,

// }

// impl<'a> SnapshotItem<'a> {
//     pub const fn new() -> Self {
//         let primary = Keymap::new(b"primary");
//         let snapshots = Keymap::new(b"snapshots");
//         Self { primary, snapshots }
//     }

//     pub fn save(
//         &mut self,
//         store: &mut dyn Storage,
//         height: u64,
//         value: Uint128,
//     ) -> StdResult<()> {
//         // Save the old value to snapshots
//         let snapshotvalue = self.primary.get(store, &(height-1));
//         let _ = self.snapshots.insert(store, &height, &snapshotvalue.unwrap());

//         // Save the new value to primary
//         let _ = self.primary.insert(store, &height, &value);

//         Ok(())
//     }

//     pub fn load(&self, store: &dyn Storage, height: Option<u64>) -> Uint128 {
        

//         // Check if there is a snapshotted value at the provided height
//          let snapshot_value = self.snapshots.get(store, &height.unwrap()).unwrap();

//          if Some(snapshot_value).is_some(){
//             snapshot_value
//          }
//          else {
//             // If not, return the current value from primary or zero if key is not found.
//             self.primary.get(store, &height.unwrap()).unwrap_or_default()
//         }
             
        
//     }
// }


