use dao_interface::state::AnyContractInfo;
use secret_storage_plus::Item;

pub const DAO: Item<AnyContractInfo> = Item::new("dao");
pub const TOKEN: Item<AnyContractInfo> = Item::new("token");
