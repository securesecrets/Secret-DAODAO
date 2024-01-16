use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use secret_storage_plus::Item;

#[cw_serde]
pub struct Config {
    pub staking_addr: Addr,
    pub staking_code_hash: String,
    pub reward_rate: Uint128,
    pub reward_token: Addr,
    pub reward_token_code_hash: String,
    pub reward_distributor_viewing_key: String,
}

// `"config"` key stores v1 configuration.
pub const CONFIG: Item<Config> = Item::new("config_v2");

pub const LAST_PAYMENT_BLOCK: Item<u64> = Item::new("last_payment_block");
