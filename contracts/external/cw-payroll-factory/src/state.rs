use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use secret_storage_plus::Item;
use secret_toolkit::storage::Keymap;

#[cw_serde]
pub struct VestingContract {
    pub contract: String,
    pub instantiator: String,
    pub recipient: String,
}

#[cw_serde]
pub struct VestingContractInstantiateInfo {
    pub code_id: u64,
    pub code_hash: String,
}

/// Temporarily holds the address of the instantiator for use in submessages
pub const TMP_INSTANTIATOR_INFO: Item<Addr> = Item::new("tmp_instantiator_info");
pub const VESTING_INFO: Item<VestingContractInstantiateInfo> = Item::new("pci");
pub static VESTING_CONTRACTS: Keymap<Addr, VestingContract> = Keymap::new(b"vc");
