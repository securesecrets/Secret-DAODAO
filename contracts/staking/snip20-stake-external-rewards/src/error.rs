use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error(transparent)]
    Std(#[from] StdError),
    #[error(transparent)]
    Ownable(#[from] cw_ownable::OwnershipError),
    #[error(transparent)]
    Snip20Error(#[from] Snip20ContractError),
    #[error("Staking change hook sender is not staking contract")]
    InvalidHookSender {},
    #[error("No rewards claimable")]
    NoRewardsClaimable {},
    #[error("Reward period not finished")]
    RewardPeriodNotFinished {},
    #[error("Invalid funds")]
    InvalidFunds {},
    #[error("Invalid Snip20")]
    InvalidSnip20 {},
    #[error("Reward rate less then one per block")]
    RewardRateLessThenOnePerBlock {},
    #[error("Reward duration can not be zero")]
    ZeroRewardDuration {},
    #[error("can not migrate. current version is up to date")]
    AlreadyMigrated {},
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
