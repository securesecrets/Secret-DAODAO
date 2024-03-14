use cosmwasm_std::Addr;
use secret_storage_plus::Item;

pub const DAO: Item<Addr> = Item::new("dao");
pub const TOKEN: Item<Addr> = Item::new("token");
