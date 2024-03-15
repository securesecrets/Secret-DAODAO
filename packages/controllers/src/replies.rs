use thiserror::Error;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{StdError, Storage};

use secret_toolkit::{
    serialization::Json,
    storage::{Item, Keymap},
};

#[derive(Error, Debug, PartialEq)]
pub enum ReplyError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Given id doesn't exist")]
    ReplyNotRegistered {},
}

#[cw_serde]
pub enum ReplyEvent {
    VotingModuleInstantiate { code_hash: String },
    ProposalModuleInstantiate { code_hash: String },
    PreProposalModuleInstantiate { code_hash: String },
    Snip20ModuleInstantiate { code_hash: String },
    Snip20ModuleCreateViewingKey {},
    FailedPreProposeModuleHook {},
    FailedVoteHook { idx: u64 },
    FailedProposalHook { idx: u64 },
    FailedProposalExecution { proposal_id: u64 },
}
// store all hook addresses in one item. We cannot have many of them before the contract becomes unusable anyway.
pub struct ReplyIds<'a> {
    keys: Keymap<'a, u64, ReplyEvent, Json>,
    curr_id: Item<'a, u64>,
}

impl<'a> ReplyIds<'a> {
    pub const fn new(namespace: &'a [u8], count_namespace: &'a [u8]) -> Self {
        ReplyIds {
            keys: Keymap::new(namespace),
            curr_id: Item::new(count_namespace),
        }
    }

    pub fn add_event(
        &self,
        storage: &mut dyn Storage,
        event: ReplyEvent,
    ) -> Result<u64, ReplyError> {
        let next_id = self
            .curr_id
            .load(storage)
            .unwrap_or_default()
            .wrapping_add(1);
        self.keys.insert(storage, &next_id, &event)?;
        self.curr_id.save(storage, &next_id)?;
        Ok(next_id)
    }

    pub fn get_event(&self, storage: &mut dyn Storage, id: u64) -> Result<ReplyEvent, ReplyError> {
        match self.keys.get(storage, &id) {
            Some(event) => Ok(event),
            None => Err(ReplyError::ReplyNotRegistered {}),
        }
    }
}
