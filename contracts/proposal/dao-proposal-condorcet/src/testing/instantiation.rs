use cosmwasm_std::{to_binary, Addr, ContractInfo, Decimal};
use dao_voting::threshold::PercentageThreshold;
use secret_multi_test::{App, Executor};
use secret_utils::Duration;

use crate::config::UncheckedConfig;

use super::{contracts::query_auth_contract, suite::SuiteBuilder};

#[test]
fn test_instantiation() {
    let default_config = SuiteBuilder::default().instantiate;

    let suite = SuiteBuilder::default().build();
    let config = suite.query_config();

    assert_eq!(config, default_config.into_checked().unwrap())
}

#[test]
#[should_panic(expected = "Min voting period must be less than or equal to max voting period")]
fn test_instantiate_conflicting_proposal_durations() {
    SuiteBuilder::with_config(UncheckedConfig {
        quorum: PercentageThreshold::Percent(Decimal::percent(15)),
        voting_period: Duration::Height(10),
        min_voting_period: Some(Duration::Height(11)),
        close_proposals_on_execution_failure: true,
        dao_code_hash: "dao_code_hash".to_string(),
    })
    .build();
}

#[test]
#[should_panic(
    expected = "min_voting_period and max_voting_period must have the same units (height or time)"
)]
fn test_instantiate_conflicting_duration_types() {
    SuiteBuilder::with_config(UncheckedConfig {
        quorum: PercentageThreshold::Percent(Decimal::percent(15)),
        voting_period: Duration::Height(10),
        min_voting_period: Some(Duration::Time(9)),
        close_proposals_on_execution_failure: true,
        dao_code_hash: "dao_code_hash".to_string(),
    })
    .build();
}

#[test]
fn test_instantiate_open_til_expiry() {
    SuiteBuilder::with_config(UncheckedConfig {
        quorum: PercentageThreshold::Percent(Decimal::percent(15)),
        voting_period: Duration::Height(10),
        min_voting_period: Some(Duration::Height(10)),
        close_proposals_on_execution_failure: true,
        dao_code_hash: "dao_code_hash".to_string(),
    })
    .build();
    SuiteBuilder::with_config(UncheckedConfig {
        quorum: PercentageThreshold::Percent(Decimal::percent(15)),
        voting_period: Duration::Time(10),
        min_voting_period: Some(Duration::Time(10)),
        close_proposals_on_execution_failure: true,
        dao_code_hash: "dao_code_hash".to_string(),
    })
    .build();
}

pub(crate) fn instantiate_query_auth(app: &mut App) -> ContractInfo {
    let query_auth_info = app.store_code(query_auth_contract());
    let msg = shade_protocol::contract_interfaces::query_auth::InstantiateMsg {
        admin_auth: shade_protocol::Contract {
            address: Addr::unchecked("admin_contract"),
            code_hash: "code_hash".to_string(),
        },
        prng_seed: to_binary("seed").unwrap(),
    };

    app.instantiate_contract(
        query_auth_info,
        Addr::unchecked("creator"),
        &msg,
        &[],
        "query_auth",
        None,
    )
    .unwrap()
}
