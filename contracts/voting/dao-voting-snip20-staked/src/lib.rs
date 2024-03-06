#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]

pub mod contract;
mod error;
pub mod msg;
pub mod snip20_msg;
pub mod snip20_stake_msg;
pub mod state;

#[cfg(test)]
mod tests;

pub use crate::error::ContractError;
