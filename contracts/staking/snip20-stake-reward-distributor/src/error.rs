use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error(transparent)]
    Std(#[from] StdError),

    #[error(transparent)]
    Ownership(#[from] cw_ownable::OwnershipError),

    #[error("Invalid Cw20")]
    InvalidSnip20 {},

    #[error("Invalid Staking Contract")]
    InvalidStakingContract {},

    #[error("Zero eligible rewards")]
    ZeroRewards {},

    #[error("Rewards have already been distributed for this block")]
    RewardsDistributedForBlock {},

    #[error("can not migrate. current version is up to date")]
    AlreadyMigrated {},

    #[error("Error in executing token contract")]
    TokenExecuteError {},

    #[error("Got a submessage reply with unknown id: {id}")]
    UnknownReplyId { id: u64 },
}
