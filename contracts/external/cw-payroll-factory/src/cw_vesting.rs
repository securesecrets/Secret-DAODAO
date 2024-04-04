use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Timestamp, Uint128};
use cw_denom::UncheckedDenom;
use cw_vesting::vesting::Schedule;
use secret_toolkit::utils::InitCallback;

#[cw_serde]
pub struct PayrollInstantiateMsg {
    /// The optional owner address of the contract. If an owner is
    /// specified, the owner may cancel the vesting contract at any
    /// time and withdraw unvested funds.
    pub owner: Option<String>,
    /// The receiver address of the vesting tokens.
    pub recipient: String,

    /// The a name or title for this payment.
    pub title: String,
    /// A description for the payment to provide more context.
    pub description: Option<String>,

    /// The total amount of tokens to be vested.
    pub total: Uint128,
    /// The type and denom of token being vested.
    pub denom: UncheckedDenom,

    /// The vesting schedule, can be either `SaturatingLinear` vesting
    /// (which vests evenly over time), or `PiecewiseLinear` which can
    /// represent a more complicated vesting schedule.
    pub schedule: Schedule,
    /// The time to start vesting, or None to start vesting when the
    /// contract is instantiated. `start_time` may be in the past,
    /// though the contract checks that `start_time +
    /// vesting_duration_seconds > now`. Otherwise, this would amount
    /// to a regular fund transfer.
    pub start_time: Option<Timestamp>,
    /// The length of the vesting schedule in seconds. Must be
    /// non-zero, though one second vesting durations are
    /// allowed. This may be combined with a `start_time` in the
    /// future to create an agreement that instantly vests at a time
    /// in the future, and allows the receiver to stake vesting tokens
    /// before the agreement completes.
    ///
    /// See `suite_tests/tests.rs`
    /// `test_almost_instavest_in_the_future` for an example of this.
    pub vesting_duration_seconds: u64,

    /// The unbonding duration for the chain this contract is deployed
    /// on. Smart contracts do not have access to this data as
    /// stargate queries are disabled on most chains, and cosmwasm-std
    /// provides no way to query it.
    ///
    /// This value being too high will cause this contract to hold
    /// funds for longer than needed, this value being too low will
    /// reduce the quality of error messages and require additional
    /// external calculations with correct values to withdraw
    /// avaliable funds from the contract.
    pub unbonding_duration_seconds: u64,
}

impl InitCallback for PayrollInstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}
