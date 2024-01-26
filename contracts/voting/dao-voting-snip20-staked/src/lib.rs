#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]

pub mod contract;
mod error;
pub mod msg;
pub mod snip20_msg;
pub mod state;
pub mod snip20_stake_msg;

#[cfg(test)]
mod tests;

pub use crate::error::ContractError;
