use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum RolesContractError {
    #[error(transparent)]
    Std(#[from] StdError),

    #[error("Nft does not exist ")]
    NftDoesNotExist {},

    #[error(transparent)]
    Base(#[from] cw721_base::ContractError),

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
}
