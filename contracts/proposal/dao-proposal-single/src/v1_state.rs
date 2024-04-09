//! Helper methods for migrating from v1 to v2 state. These will need
//! to be updated when we bump our CosmWasm version for v2.

use dao_voting::{
    status::Status,
    threshold::{PercentageThreshold, Threshold},
    voting::Votes,
};
use secret_utils::{Duration, Expiration};

pub fn v1_percentage_threshold_to_v2(
    v1: dao_voting::threshold::PercentageThreshold,
) -> PercentageThreshold {
    match v1 {
        dao_voting::threshold::PercentageThreshold::Majority {} => PercentageThreshold::Majority {},
        dao_voting::threshold::PercentageThreshold::Percent(p) => PercentageThreshold::Percent(p),
    }
}

pub fn v1_threshold_to_v2(v1: dao_voting::threshold::Threshold) -> Threshold {
    match v1 {
        dao_voting::threshold::Threshold::AbsolutePercentage { percentage } => {
            Threshold::AbsolutePercentage {
                percentage: v1_percentage_threshold_to_v2(percentage),
            }
        }
        dao_voting::threshold::Threshold::ThresholdQuorum { threshold, quorum } => {
            Threshold::ThresholdQuorum {
                threshold: v1_percentage_threshold_to_v2(threshold),
                quorum: v1_percentage_threshold_to_v2(quorum),
            }
        }
        dao_voting::threshold::Threshold::AbsoluteCount { threshold } => {
            Threshold::AbsoluteCount { threshold }
        }
    }
}

pub fn v1_duration_to_v2(v1: secret_utils::Duration) -> Duration {
    match v1 {
        secret_utils::Duration::Height(height) => Duration::Height(height),
        secret_utils::Duration::Time(time) => Duration::Time(time),
    }
}

pub fn v1_expiration_to_v2(v1: secret_utils::Expiration) -> Expiration {
    match v1 {
        secret_utils::Expiration::AtHeight(height) => Expiration::AtHeight(height),
        secret_utils::Expiration::AtTime(time) => Expiration::AtTime(time),
        secret_utils::Expiration::Never {} => Expiration::Never {},
    }
}

pub fn v1_votes_to_v2(v1: dao_voting::voting::Votes) -> Votes {
    Votes {
        yes: v1.yes,
        no: v1.no,
        abstain: v1.abstain,
    }
}

pub fn v1_status_to_v2(v1: dao_voting::status::Status) -> Status {
    match v1 {
        Status::Open => Status::Open,
        Status::Rejected => Status::Rejected,
        Status::Passed => Status::Passed,
        Status::Executed => Status::Executed,
        Status::Closed => Status::Closed,
        Status::ExecutionFailed => Status::ExecutionFailed,
        Status::VetoTimelock { expiration } => Status::VetoTimelock { expiration },
        Status::Vetoed => Status::Vetoed,
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{Decimal, Timestamp, Uint128};

    use super::*;

    #[test]
    fn test_percentage_conversion() {
        assert_eq!(
            v1_percentage_threshold_to_v2(dao_voting::threshold::PercentageThreshold::Majority {}),
            PercentageThreshold::Majority {}
        );
        assert_eq!(
            v1_percentage_threshold_to_v2(dao_voting::threshold::PercentageThreshold::Percent(
                Decimal::percent(80)
            )),
            PercentageThreshold::Percent(Decimal::percent(80))
        )
    }

    #[test]
    fn test_duration_conversion() {
        assert_eq!(
            v1_duration_to_v2(secret_utils::Duration::Height(100)),
            Duration::Height(100)
        );
        assert_eq!(
            v1_duration_to_v2(secret_utils::Duration::Time(100)),
            Duration::Time(100)
        );
    }

    #[test]
    fn test_expiration_conversion() {
        assert_eq!(
            v1_expiration_to_v2(secret_utils::Expiration::AtHeight(100)),
            Expiration::AtHeight(100)
        );
        assert_eq!(
            v1_expiration_to_v2(secret_utils::Expiration::AtTime(Timestamp::from_seconds(
                100
            ))),
            Expiration::AtTime(Timestamp::from_seconds(100))
        );
        assert_eq!(
            v1_expiration_to_v2(secret_utils::Expiration::Never {}),
            Expiration::Never {}
        );
    }

    #[test]
    fn test_threshold_conversion() {
        assert_eq!(
            v1_threshold_to_v2(dao_voting::threshold::Threshold::AbsoluteCount {
                threshold: Uint128::new(10)
            }),
            Threshold::AbsoluteCount {
                threshold: Uint128::new(10)
            }
        );
        assert_eq!(
            v1_threshold_to_v2(dao_voting::threshold::Threshold::AbsolutePercentage {
                percentage: dao_voting::threshold::PercentageThreshold::Majority {}
            }),
            Threshold::AbsolutePercentage {
                percentage: PercentageThreshold::Majority {}
            }
        );
        assert_eq!(
            v1_threshold_to_v2(dao_voting::threshold::Threshold::ThresholdQuorum {
                threshold: dao_voting::threshold::PercentageThreshold::Majority {},
                quorum: dao_voting::threshold::PercentageThreshold::Percent(Decimal::percent(20))
            }),
            Threshold::ThresholdQuorum {
                threshold: PercentageThreshold::Majority {},
                quorum: PercentageThreshold::Percent(Decimal::percent(20))
            }
        );
    }

    #[test]
    fn test_status_conversion() {
        macro_rules! status_conversion {
            ($x:expr) => {
                assert_eq!(
                    v1_status_to_v2({
                        use dao_voting::status::Status;
                        $x
                    }),
                    $x
                )
            };
        }

        status_conversion!(Status::Open);
        status_conversion!(Status::Closed);
        status_conversion!(Status::Executed);
        status_conversion!(Status::Rejected)
    }
}
