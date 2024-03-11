use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use dao_interface::state::AnyContractInfo;
use secret_storage_plus::Item;

use crate::msg::NftMintMsg;

#[cw_serde]
pub struct Config {
    pub nft_address: Addr,
    pub nft_code_hash: String,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const DAO: Item<AnyContractInfo> = Item::new("dao");

// Holds initial NFTs messages during instantiation.
pub const INITIAL_NFTS: Item<Vec<NftMintMsg>> = Item::new("initial_nfts");
