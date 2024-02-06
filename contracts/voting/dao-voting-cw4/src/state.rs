use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use secret_storage_plus::Item;

#[cw_serde]
pub struct Config {
    pub group_contract_address: String,
    pub group_contract_code_hash: String,
}

pub const GROUP_CONTRACT: Item<Config> = Item::new("group_contract");
pub const DAO: Item<Addr> = Item::new("dao_address");
