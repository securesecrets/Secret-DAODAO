use dao_voting::{
    status::{self, Status},
    threshold::{self, PercentageThreshold, Threshold},
    voting::Votes,
};
use secret_utils::Expiration;

pub(crate) fn v1_expiration_to_v2(v1: secret_utils::Expiration) -> Expiration {
    match v1 {
        secret_utils::Expiration::AtHeight(height) => Expiration::AtHeight(height),
        secret_utils::Expiration::AtTime(time) => Expiration::AtTime(time),
        secret_utils::Expiration::Never {} => Expiration::Never {},
    }
}

pub(crate) fn v1_percentage_threshold_to_v2(
    v1: threshold::PercentageThreshold,
) -> PercentageThreshold {
    match v1 {
        threshold::PercentageThreshold::Majority {} => PercentageThreshold::Majority {},
        threshold::PercentageThreshold::Percent(p) => PercentageThreshold::Percent(p),
    }
}

pub(crate) fn v1_threshold_to_v2(v1: threshold::Threshold) -> Threshold {
    match v1 {
        threshold::Threshold::AbsolutePercentage { percentage } => Threshold::AbsolutePercentage {
            percentage: v1_percentage_threshold_to_v2(percentage),
        },
        threshold::Threshold::ThresholdQuorum { threshold, quorum } => Threshold::ThresholdQuorum {
            threshold: v1_percentage_threshold_to_v2(threshold),
            quorum: v1_percentage_threshold_to_v2(quorum),
        },
        threshold::Threshold::AbsoluteCount { threshold } => Threshold::AbsoluteCount { threshold },
    }
}

pub(crate) fn v1_status_to_v2(v1: status::Status) -> Status {
    match v1 {
        status::Status::Open => Status::Open,
        status::Status::Rejected => Status::Rejected,
        status::Status::Passed => Status::Passed,
        status::Status::Executed => Status::Executed,
        status::Status::Closed => Status::Closed,
        Status::ExecutionFailed => Status::ExecutionFailed,
        Status::VetoTimelock { expiration } => Status::VetoTimelock { expiration },
        Status::Vetoed => Status::Vetoed,
    }
}

pub(crate) fn v1_votes_to_v2(v1: Votes) -> Votes {
    Votes {
        yes: v1.yes,
        no: v1.no,
        abstain: v1.abstain,
    }
}
