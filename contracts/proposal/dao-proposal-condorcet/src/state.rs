use cosmwasm_std::{Addr, StdResult, Storage};
use dao_interface::state::AnyContractInfo;
use secret_cw_controllers::ReplyIds;
use secret_storage_plus::Item;
use secret_toolkit::{serialization::Json, storage::Keymap};

use crate::{config::Config, proposal::Proposal, tally::Tally, vote::Vote};

pub(crate) const DAO: Item<AnyContractInfo> = Item::new("dao");
pub(crate) const CONFIG: Item<Config> = Item::new("config");

pub(crate) static TALLY: Keymap<u32, Tally, Json> = Keymap::new(b"tallys");
pub(crate) static PROPOSAL: Keymap<u32, Proposal, Json> = Keymap::new(b"proposals");
pub(crate) static VOTE: Keymap<(u32, Addr), Vote, Json> = Keymap::new(b"votes");
pub(crate) static REPLY_IDS: ReplyIds = ReplyIds::new(b"reply_ids", b"reply_ids_count");

pub(crate) fn next_proposal_id(storage: &dyn Storage) -> StdResult<u32> {
    PROPOSAL
        .iter_keys(storage)?
        .next()
        .transpose()
        .map(|id| id.unwrap_or(0) + 1)
}
