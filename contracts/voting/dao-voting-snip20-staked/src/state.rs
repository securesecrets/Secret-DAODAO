use dao_interface::state::AnyContractInfo;
use dao_voting::threshold::ActiveThreshold;
use secret_storage_plus::Item;
use secret_utils::Duration;
use shade_protocol::utils::asset::RawContract;

pub const ACTIVE_THRESHOLD: Item<ActiveThreshold> = Item::new("active_threshold");
pub const TOKEN_CONTRACT: Item<AnyContractInfo> = Item::new("token");
pub const DAO: Item<AnyContractInfo> = Item::new("dao");
pub const STAKING_CONTRACT: Item<AnyContractInfo> = Item::new("staking_contract");
pub const STAKING_CONTRACT_UNSTAKING_DURATION: Item<Option<Duration>> =
    Item::new("staking_contract_unstaking_duration");
pub const STAKING_CONTRACT_CODE_ID: Item<u64> = Item::new("staking_contract_code_id");
pub const STAKING_CONTRACT_CODE_HASH: Item<String> = Item::new("staking_contract_code_hash");
pub const QUERY_AUTH: Item<RawContract> = Item::new("qa");
