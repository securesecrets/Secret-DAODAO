use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum RolesContractError {
    #[error(transparent)]
    Std(#[from] StdError),

    #[error("Nft does not exist ")]
    NftDoesNotExist {},

    #[error(transparent)]
    Base(#[from] Snip721ContractError),

    #[error(transparent)]
    HookError(#[from] secret_cw_controllers::HookError),

    #[error("{0}")]
    OverflowErr(#[from] OverflowError),

    #[error(transparent)]
    Ownable(#[from] cw_ownable::OwnershipError),

    #[error("Cannot burn NFT, member weight would be negative")]
    CannotBurn {},

    #[error("Would result in negative value")]
    NegativeValue {},

    #[error("The submitted weight is equal to the previous value, no change will occur")]
    NoWeightChange {},

    #[error("Unknown reply id: {id:?}")]
    UnexpectedReplyId { id: u64 },

    #[error("Custom Error val: {val:?}")]
    CustomError { val: String },
}

#[derive(Error, Debug, PartialEq)]
pub enum Snip721ContractError {
    #[error(transparent)]
    Std(#[from] StdError),

    // #[error(transparent)]
    // Ownership(#[from] OwnershipError),

    // #[error(transparent)]
    // Version(#[from] secret_cw2::VersionError),
    #[error("token_id already claimed")]
    Claimed {},

    #[error("Cannot set approval that is already expired")]
    Expired {},

    #[error("Approval not found for: {spender}")]
    ApprovalNotFound { spender: String },
}
