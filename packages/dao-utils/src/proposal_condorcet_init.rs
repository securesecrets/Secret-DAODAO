use cosmwasm_schema::cw_serde;
use dao_voting::threshold::PercentageThreshold;
use secret_toolkit::utils::InitCallback;
use secret_utils::Duration;

pub type InstantiateMsg = UncheckedConfig;

#[cw_serde]
pub struct UncheckedConfig {
    pub quorum: PercentageThreshold,
    pub voting_period: Duration,
    pub min_voting_period: Option<Duration>,
    pub close_proposals_on_execution_failure: bool,
    pub dao_code_hash: String,
}

impl InitCallback for InstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}
