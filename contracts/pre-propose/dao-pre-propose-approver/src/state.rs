use dao_interface::state::AnyContractInfo;
use secret_storage_plus::Item;
use secret_toolkit::{serialization::Json, storage::Keymap};

// Stores the address of the pre-propose approval contract
pub const PRE_PROPOSE_APPROVAL_CONTRACT: Item<AnyContractInfo> =
    Item::new("pre_propose_approval_contract");
// Maps proposal ids to pre-propose ids
pub static PROPOSAL_ID_TO_PRE_PROPOSE_ID: Keymap<u64, u64, Json> =
    Keymap::new(b"proposal_to_pre_propose");
// Maps pre-propose ids to proposal ids
pub static PRE_PROPOSE_ID_TO_PROPOSAL_ID: Keymap<u64, u64, Json> =
    Keymap::new(b"pre_propose_to_proposal");
