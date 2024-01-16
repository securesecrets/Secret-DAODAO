use cosmwasm_std::Addr;
use dao_voting::threshold::ActiveThreshold;
use secret_storage_plus::Item;
use secret_utils::Duration;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct StakingContractInfo {
    pub addr: String,
    pub code_hash: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct TokenContractInfo {
    pub addr: String,
    pub code_hash: String,
}
pub const ACTIVE_THRESHOLD: Item<ActiveThreshold> = Item::new("active_threshold");
pub const TOKEN_CONTRACT: Item<TokenContractInfo> = Item::new("token");
pub const DAO: Item<Addr> = Item::new("dao");
pub const STAKING_CONTRACT: Item<StakingContractInfo> = Item::new("staking_contract");
pub const STAKING_CONTRACT_UNSTAKING_DURATION: Item<Option<Duration>> =
    Item::new("staking_contract_unstaking_duration");
pub const STAKING_CONTRACT_CODE_ID: Item<u64> = Item::new("staking_contract_code_id");
