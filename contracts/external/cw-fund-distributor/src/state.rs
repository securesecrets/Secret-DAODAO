use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use secret_storage_plus::Item;
use secret_toolkit::storage::Keymap;
use secret_utils::Expiration;
use shade_protocol::Contract;

#[cw_serde]
pub struct VotingContractInfo {
    pub address: Addr,
    pub code_hash: String,
}

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub query_auth: Contract,
}
/// block height for distribution snapshot
pub const DISTRIBUTION_HEIGHT: Item<u64> = Item::new("distribution_height");
/// period during which the contract can be funded
/// exclusive of the expiration block
pub const FUNDING_PERIOD_EXPIRATION: Item<Expiration> = Item::new("funding_period");
/// voting contract to determine the voting power
pub const VOTING_CONTRACT: Item<VotingContractInfo> = Item::new("voting_contract");
/// total voting power at the distribution height
pub const TOTAL_POWER: Item<Uint128> = Item::new("total_power");

/// maps token address to the amount being distributed
pub static SNIP20_BALANCES: Keymap<Addr, Uint128> = Keymap::new(b"snip20_balances");
pub static NATIVE_BALANCES: Keymap<String, Uint128> = Keymap::new(b"native_balances");

/// maps (ADDRESS, TOKEN_ADDRESS) to amounts
/// that have been claimed by the address
pub static SNIP20_CLAIMS: Keymap<(Addr, Addr), Uint128> = Keymap::new(b"snip20_claims");
/// maps (ADDRESS, NATIVE_DENOM) to amounts
/// that have been claimed by the address
pub static NATIVE_CLAIMS: Keymap<(Addr, String), Uint128> = Keymap::new(b"native_claims");

pub static SNIP20S_CODE_HASH: Keymap<Addr, String> = Keymap::new(b"snip20s_code_hash");

pub const CONFIG: Item<Config> = Item::new("config");
