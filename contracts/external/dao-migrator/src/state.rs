use dao_interface::state::AnyContractInfo;
use secret_storage_plus::Item;

use crate::types::{ModulesAddrs, TestState};

/// Holds data about the DAO before migration (so we can test against it after migration)
pub const TEST_STATE: Item<TestState> = Item::new("test_state");
/// Holds addresses for what we need to query for
pub const MODULES_ADDRS: Item<ModulesAddrs> = Item::new("module_addrs");
/// Hold the core address to be used in reply
pub const CORE_INFO: Item<AnyContractInfo> = Item::new("core_addr");
