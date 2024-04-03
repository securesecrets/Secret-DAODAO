use secret_storage_plus::Item;

use crate::vesting::Payment;

pub const PAYMENT: Payment = Payment::new("vesting", b"staked", b"validator", b"cardinality");
pub const UNBONDING_DURATION_SECONDS: Item<u64> = Item::new("ubs");
