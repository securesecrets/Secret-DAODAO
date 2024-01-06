use cosmwasm_std::{Addr, StdError};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error(transparent)]
    Std(#[from] StdError),

    #[error(transparent)]
    Snip20Error(#[from] Snip20ContractError),

    #[error(transparent)]
    Ownership(#[from] cw_ownable::OwnershipError),

    #[error(transparent)]
    HookError(#[from] cw_hooks::HookError),

    #[error(transparent)]
    UnstakingDurationError(#[from] dao_voting::duration::UnstakingDurationError),

    #[error("can not migrate. current version is up to date")]
    AlreadyMigrated {},

    #[error("Unstaking this amount violates the invariant: (snip20 total_supply <= 2^128)")]
    Snip20InvaraintViolation {},

    #[error("Can not unstake more than has been staked")]
    ImpossibleUnstake {},

    #[error("Provided snip20 errored in response to TokenInfo query")]
    InvalidSnip20 {},

    #[error("Invalid token")]
    InvalidToken { received: Addr, expected: Addr },

    #[error("Nothing to claim")]
    NothingToClaim {},

    #[error("Nothing to unstake")]
    NothingStaked {},

    #[error("Too many outstanding claims. Claim some tokens before unstaking more.")]
    TooManyClaims {},
}

#[derive(Error, Debug, PartialEq)]
pub enum Snip20ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Cannot set to own account")]
    CannotSetOwnAccount {},

    // Unused error case. Zero is now treated like every other value.
    #[deprecated(note = "Unused. All zero amount checks have been removed")]
    #[error("Invalid zero amount")]
    InvalidZeroAmount {},

    #[error("Allowance is expired")]
    Expired {},

    #[error("No allowance for this account")]
    NoAllowance {},

    #[error("Minting cannot exceed the cap")]
    CannotExceedCap {},

    #[error("Logo binary data exceeds 5KB limit")]
    LogoTooBig {},

    #[error("Invalid xml preamble for SVG")]
    InvalidXmlPreamble {},

    #[error("Invalid png header")]
    InvalidPngHeader {},

    #[error("Invalid expiration value")]
    InvalidExpiration {},

    #[error("Duplicate initial balance addresses")]
    DuplicateInitialBalanceAddresses {},
}
