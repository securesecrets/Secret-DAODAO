#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]

use std::{
    fmt::Debug,
    marker::PhantomData,
    ops::{Add, Sub},
};

use serde::de::DeserializeOwned;
use serde::Serialize;

use cosmwasm_std::{StdResult, Storage};
use secret_toolkit::storage::Keymap;

/// A map that ensures that the gas cost of updating a value is higher
/// than the cost of loading a value and allows updating values in the
/// future. The cost of loading a value from this map is O(1) in gas.
///
/// This map has a special high-performance case if it is being used
/// to track unbonding tokens. In that case, the runtime to update a
/// key is O(# times unbonding duration has changed). For a proof of
/// this, and further runtime analysis see [this
/// essay](https://gist.github.com/0xekez/15fab6436ed593cbd59f0bdf7ecf1f61).
///
/// # Example
///
/// ```
/// # use cosmwasm_std::{testing::mock_dependencies, Uint128};
/// # use cw_wormhole::Wormhole;
/// let storage = &mut mock_dependencies().storage;
/// let fm: Wormhole<String, Uint128> = Wormhole::new(b"ns");
///
/// fm.increment(storage, "fm".to_string(), 10, Uint128::new(1))
///     .unwrap();
/// fm.increment(storage, "fm".to_string(), 9, Uint128::new(2))
///     .unwrap();
///
/// // no value exists at time=8
/// assert_eq!(fm.load(storage, "fm".to_string(), 8).unwrap(), None);
/// // value was incremented by 2 at time=9
/// assert_eq!(
///     fm.load(storage, "fm".to_string(), 9).unwrap(),
///     Some(Uint128::new(2))
/// );
/// // value was incremented by 1 at time=10 making final value 3
/// assert_eq!(
///     fm.load(storage, "fm".to_string(), 10).unwrap(),
///     Some(Uint128::new(3))
/// );
/// ```
pub struct Wormhole<'n, K, V> {
    namespace: &'n [u8],
    k: PhantomData<K>,
    v: PhantomData<V>,
}

impl<'n, K, V> Wormhole<'n, K, V> {
    /// Creates a new map using the provided namespace.
    ///
    /// The namespace identifies the prefix in the SDK's prefix
    /// store that values and keys will be stored under.
    ///
    /// # Example
    ///
    /// ```
    /// # use cw_wormhole::Wormhole;
    /// # use cosmwasm_std::{Addr, Uint128};
    ///
    /// pub const MAP: Wormhole<&Addr, Uint128> = Wormhole::new(b"unbonded_balances");
    /// ```
    pub const fn new(namespace: &'n [u8]) -> Self {
        Self {
            namespace,
            k: PhantomData,
            v: PhantomData,
        }
    }
}

impl<'n, K, V> Wormhole<'n, K, V>
where
    // 1. values in the map can be serialized and deserialized
    V: serde::de::DeserializeOwned + serde::Serialize + Default + Clone + Debug,
    // 1.1. keys in the map can be cloned
    K: Serialize + DeserializeOwned + Clone + Debug,
{
    /// Loads the value at a key at the specified time. If the key has
    /// no value at that time, returns `None`. Returns `Some(value)`
    /// otherwise.
    pub fn load(&self, storage: &dyn Storage, k: K, t: u64) -> StdResult<Option<V>> {
        // Start with the highest possible time and iterate backwards until finding the first matching value
        for current_time in (0..=t).rev() {
            let key = (k.clone(), current_time);
            if let Some(value) = self.snapshots().get(storage, &key) {
                return Ok(Some(value));
            }
        }
        // If no matching value is found, return None
        Ok(None)
    }

    /// Increments the value of key `k` at time `t` by amount `i`.
    pub fn increment(&self, storage: &mut dyn Storage, k: K, t: u64, i: V) -> StdResult<V>
    where
        V: Add<Output = V>,
    {
        self.update(storage, k, t, &mut |v, _| v + i.clone())
    }

    /// Decrements the value of key `k` at time `t` by amount `i`.
    pub fn decrement(&self, storage: &mut dyn Storage, k: K, t: u64, i: V) -> StdResult<V>
    where
        V: Sub<Output = V>,
    {
        self.update(storage, k, t, &mut |v, _| v - i.clone())
    }

    /// Gets the snapshot map with a namespace with a lifetime equal
    /// to the lifetime of `&'a self`.
    const fn snapshots(&self) -> Keymap<'_, (K, u64), V> {
        Keymap::new(self.namespace)
    }

    /// Updates `k` at time `t`. To do so, update is called on the
    /// current value of `k` (or Default::default() if there is no
    /// current value), and then all future (t' > t) values of `k`.
    ///
    /// For example, to perform a increment operation, the `update`
    /// function used is `|v| v + amount`.
    ///
    /// The new value at `t` is returned.
    pub fn update(
        &self,
        storage: &mut dyn Storage,
        k: K,
        t: u64,
        update: &mut dyn FnMut(V, u64) -> V,
    ) -> StdResult<V> {
        let prev = self.load(storage, k.clone(), t)?.unwrap_or_default();
        let updated = update(prev.clone(), t);

        // Update the value at time t
        self.snapshots()
            .insert(storage, &(k.clone(), t), &updated)?;

        // Update all future times t' > t
        for t_prime in (t + 1).. {
            let key = (k.clone(), t_prime);
            match self.snapshots().get(storage, &key) {
                Some(mut value) => {
                    value = update(value, t_prime);
                    self.snapshots().insert(storage, &key, &value)?;
                }
                None => break,
            }
        }

        Ok(updated)
    }

    /// Updates a single key `k` at time `t` without performing an
    /// update on values of `(k, t')` where `t' > t`.
    ///
    /// This is safe to use if updating a the key at the specified
    /// time is not expected to impact values of the key \forall t' >
    /// t. If you want to update a key and also update future values
    /// of that key, (which is likely what you normally want) use the
    /// `update` method.
    ///
    /// ```text
    ///                         Unbonding Slash (Tokens / Time)
    /// 30 +------------------------------------------------------------------+
    ///    |            +             +            +             +            |
    ///    |                                                w/o slash +.....+ |
    /// 25 |-+                                               w/ slash =======-|
    ///    |                                                                  |
    ///    |                                                                  |
    ///    |                                                                  |
    /// 20 |===========================............+.............+          +-|
    ///    |                          =                          :            |
    ///    |                          =                          :            |
    /// 15 |-+                        ============================          +-|
    ///    |                                                     =            |
    ///    |                                                     =            |
    ///    |                                                     =            |
    /// 10 |-+                                                   =============|
    ///    |                                                                  |
    ///    |            +             +            +             +            |
    ///  5 +------------------------------------------------------------------+
    ///    0            1             2            3             4            5
    ///    ^                          ^                          ^
    ///    |                          |                          |
    ///  Unbonding Start            Slash                      Unbonded
    ///
    ///                                   Time ->
    /// ```
    ///
    /// For example, consider the above graph showing bonded +
    /// unbonded tokens over time with a slash ocuring at `t=2`. In
    /// this case, the slash does not impact the value at `t=4` (when
    /// unbonding completes), but it does change intermediate values,
    /// so it is safe to use `dangerously_update` to register the
    /// slash at t=2.
    pub fn dangerously_update(
        &self,
        storage: &mut dyn Storage,
        k: K,
        t: u64,
        update: &mut dyn FnMut(V, u64) -> V,
    ) -> StdResult<V> {
        let prev = self.load(storage, k.clone(), t)?.unwrap_or_default();
        let updated = update(prev.clone(), t);
        self.snapshots().insert(storage, &(k, t), &updated)?;
        Ok(updated)
    }
}

#[cfg(test)]
mod tests;
