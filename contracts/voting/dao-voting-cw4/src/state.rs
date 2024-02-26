use dao_interface::state::AnyContractInfo;
use secret_storage_plus::Item;

pub const GROUP_CONTRACT: Item<AnyContractInfo> = Item::new("group_contract");
pub const DAO: Item<AnyContractInfo> = Item::new("dao_address");
