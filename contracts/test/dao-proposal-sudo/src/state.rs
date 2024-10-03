use cosmwasm_std::Addr;
use dao_interface::state::AnyContractInfo;
use secret_storage_plus::Item;

pub const ROOT: Item<Addr> = Item::new("root");
pub const DAO: Item<AnyContractInfo> = Item::new("dao");
