use crate::msg::InstantiateMsg;
use crate::testing::execute::create_snip20_viewing_key;
use crate::testing::instantiate::get_pre_propose_info;
use crate::testing::{
    execute::{
        close_proposal, execute_proposal, execute_proposal_should_fail, make_proposal,
        mint_snip20s, vote_on_proposal,
    },
    instantiate::{
        get_default_token_dao_proposal_module_instantiate,
        instantiate_with_staked_balances_governance,
    },
    queries::{query_balance_cw20, query_dao_token, query_proposal, query_single_proposal_module},
};
use cosmwasm_std::testing::mock_info;
use cosmwasm_std::{to_binary, ContractInfo, CosmosMsg, Decimal, Uint128, WasmMsg};
use dao_interface::msg::InitialBalance;
use dao_interface::state::AnyContractInfo;
use dao_voting::{
    deposit::{DepositRefundPolicy, UncheckedDepositInfo, VotingModuleTokenType},
    status::Status,
    threshold::{PercentageThreshold, Threshold::AbsolutePercentage},
    voting::Vote,
};
use secret_multi_test::{next_block, App};
use secret_utils::Duration;

use super::execute::create_viewing_key;
use super::CREATOR_ADDR;
use crate::{query::ProposalResponse, ContractError};

struct CommonTest {
    app: App,
    proposal_module: AnyContractInfo,
    proposal_id: u64,
    query_auth_info: AnyContractInfo,
}
fn setup_test(messages: Vec<CosmosMsg>) -> CommonTest {
    let mut app = App::default();
    let instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    let core_contract_info =
        instantiate_with_staked_balances_governance(&mut app, instantiate, None);
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );

    let query_auth_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &dao_interface::msg::QueryMsg::QueryAuthInfo {},
        )
        .unwrap();

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info(CREATOR_ADDR, &[]),
    );
    // Mint some tokens to pay the proposal deposit.
    mint_snip20s(
        &mut app,
        &gov_token_info.addr.clone(),
        gov_token_info.code_hash.clone(),
        &core_contract_info.address.clone(),
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_owned(),
        },
        messages,
    );

    CommonTest {
        app,
        proposal_module,
        proposal_id,
        query_auth_info,
    }
}

// A proposal that is still accepting votes (is open) cannot
// be executed. Any attempts to do so should fail and return
// an error.
#[test]
fn test_execute_proposal_open() {
    let CommonTest {
        mut app,
        proposal_module,
        proposal_id,
        query_auth_info,
    } = setup_test(vec![]);

    app.update_block(next_block);

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr,
            code_hash: query_auth_info.code_hash,
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    // assert proposal is open
    let proposal = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Open);

    // attempt to execute and assert that it fails
    let err = execute_proposal_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash,
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_owned(),
        },
        proposal_id,
    );
    assert!(matches!(err, ContractError::NotPassed {}))
}

// A proposal can be executed if and only if it passed.
// Any attempts to execute a proposal that has been rejected
// or closed (after rejection) should fail and return an error.
#[test]
fn test_execute_proposal_rejected_closed() {
    let CommonTest {
        mut app,
        proposal_module,
        proposal_id,
        query_auth_info,
    } = setup_test(vec![]);

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr,
            code_hash: query_auth_info.code_hash,
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    // Assert proposal is open and vote enough to reject it
    let proposal: ProposalResponse = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        1,
    );
    assert_eq!(proposal.proposal.status, Status::Open);
    vote_on_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_owned(),
        },
        Vote::No,
    );

    app.update_block(next_block);

    // Assert proposal is rejected
    let proposal: ProposalResponse = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Rejected);

    // Attempt to execute
    let err = execute_proposal_should_fail(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_owned(),
        },
        proposal_id,
    );
    assert!(matches!(err, ContractError::NotPassed {}));

    app.update_block(next_block);

    // close the proposal
    close_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Closed);

    // Attempt to execute
    let err = execute_proposal_should_fail(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_owned(),
        },
        proposal_id,
    );
    assert!(matches!(err, ContractError::NotPassed {}))
}

// A proposal can only be executed once. Any subsequent
// attempts to execute it should fail and return an error.
#[test]
fn test_execute_proposal_more_than_once() {
    let CommonTest {
        mut app,
        proposal_module,
        proposal_id,
        query_auth_info,
    } = setup_test(vec![]);

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr,
            code_hash: query_auth_info.code_hash,
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    // Assert proposal is open and vote enough to reject it
    let proposal: ProposalResponse = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Open);
    vote_on_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_owned(),
        },
        Vote::Yes,
    );

    app.update_block(next_block);

    // assert proposal is passed, execute it
    let proposal = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Passed);
    execute_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_owned(),
        },
        proposal_id,
    );

    app.update_block(next_block);

    // assert proposal executed and attempt to execute it again
    let proposal = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Executed);
    let err: ContractError = execute_proposal_should_fail(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_owned(),
        },
        proposal_id,
    );
    assert!(matches!(err, ContractError::NotPassed {}));
}

// After proposal is executed, no subsequent votes
// should change the status of the proposal, even if
// the votes should shift to the opposing direction.
#[test]
pub fn test_executed_prop_state_remains_after_vote_swing() {
    let mut app = App::default();

    let instantiate = InstantiateMsg {
        veto: None,
        threshold: AbsolutePercentage {
            percentage: PercentageThreshold::Percent(Decimal::percent(15)),
        },
        max_voting_period: Duration::Time(604800), // One week.
        min_voting_period: None,
        only_members_execute: true,
        allow_revoting: false,
        pre_propose_info: get_pre_propose_info(
            &mut app,
            Some(UncheckedDepositInfo {
                denom: dao_voting::deposit::DepositToken::VotingModuleToken {
                    token_type: VotingModuleTokenType::Cw20,
                },
                amount: Uint128::new(10_000_000),
                refund_policy: DepositRefundPolicy::OnlyPassed,
            }),
            false,
        ),
        close_proposal_on_execution_failure: true,
        dao_code_hash: "".to_string(),
        query_auth: None,
    };

    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![
            InitialBalance {
                address: "threshold".to_string(),
                amount: Uint128::new(20),
            },
            InitialBalance {
                address: CREATOR_ADDR.to_string(),
                amount: Uint128::new(50),
            },
            InitialBalance {
                address: "overslept_vote".to_string(),
                amount: Uint128::new(30),
            },
        ]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );

    let query_auth_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &dao_interface::msg::QueryMsg::QueryAuthInfo {},
        )
        .unwrap();

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    let viewing_key_threshold = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("threshold", &[]),
    );

    let viewing_key_overslept_vote = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("overslept_vote", &[]),
    );

    mint_snip20s(
        &mut app,
        &gov_token_info.addr.clone(),
        gov_token_info.code_hash.clone(),
        &core_contract_info.address.clone(),
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![],
    );

    // someone quickly votes, proposal gets executed
    vote_on_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        "threshold",
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key_threshold.clone(),
            address: "threshold".into(),
        },
        Vote::Yes,
    );
    execute_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );

    app.update_block(next_block);

    // assert prop is executed prior to its expiry
    let proposal = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Executed);
    assert_eq!(proposal.proposal.votes.yes, Uint128::new(20));
    assert!(!proposal.proposal.expiration.is_expired(&app.block_info()));

    // someone wakes up and casts their vote to express their
    // opinion (not affecting the result of proposal)
    vote_on_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::No,
    );
    vote_on_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        "overslept_vote",
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key_overslept_vote.clone(),
            address: "overslept_vote".into(),
        },
        Vote::No,
    );

    app.update_block(next_block);

    // assert that everyone's votes are reflected in the proposal
    // and proposal remains in executed state
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash,
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Executed);
    assert_eq!(proposal.proposal.votes.yes, Uint128::new(20));
    assert_eq!(proposal.proposal.votes.no, Uint128::new(80));
}

// After reaching a passing state, no subsequent votes
// should change the status of the proposal, even if
// the votes should shift to the opposing direction.
#[test]
pub fn test_passed_prop_state_remains_after_vote_swing() {
    let mut app = App::default();

    let instantiate = InstantiateMsg {
        veto: None,
        threshold: AbsolutePercentage {
            percentage: PercentageThreshold::Percent(Decimal::percent(15)),
        },
        max_voting_period: Duration::Time(604800), // One week.
        min_voting_period: None,
        only_members_execute: true,
        allow_revoting: false,
        pre_propose_info: get_pre_propose_info(
            &mut app,
            Some(UncheckedDepositInfo {
                denom: dao_voting::deposit::DepositToken::VotingModuleToken {
                    token_type: VotingModuleTokenType::Cw20,
                },
                amount: Uint128::new(10_000_000),
                refund_policy: DepositRefundPolicy::OnlyPassed,
            }),
            false,
        ),
        close_proposal_on_execution_failure: true,
        dao_code_hash: "".into(),
        query_auth: None,
    };

    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![
            InitialBalance {
                address: "threshold".to_string(),
                amount: Uint128::new(20),
            },
            InitialBalance {
                address: CREATOR_ADDR.to_string(),
                amount: Uint128::new(50),
            },
            InitialBalance {
                address: "overslept_vote".to_string(),
                amount: Uint128::new(30),
            },
        ]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );

    let query_auth_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &dao_interface::msg::QueryMsg::QueryAuthInfo {},
        )
        .unwrap();

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    let viewing_key_threshold = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("threshold", &[]),
    );

    let viewing_key_overslept_vote = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("overslept_vote", &[]),
    );

    let viewing_key_token = create_snip20_viewing_key(
        &mut app,
        ContractInfo {
            address: gov_token_info.addr.clone(),
            code_hash: gov_token_info.code_hash.clone(),
        },
        mock_info("threshold", &[]),
    );

    // if the proposal passes, it should mint 100_000_000 tokens to "threshold"
    let msg = snip20_reference_impl::msg::ExecuteMsg::Mint {
        recipient: "threshold".to_string(),
        amount: Uint128::new(100_000_000),
        memo: None,
        decoys: None,
        entropy: None,
        padding: None,
    };

    let binary_msg = to_binary(&msg).unwrap();

    mint_snip20s(
        &mut app,
        &gov_token_info.addr.clone(),
        gov_token_info.code_hash.clone(),
        &core_contract_info.address.clone(),
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![WasmMsg::Execute {
            contract_addr: gov_token_info.addr.clone().to_string(),
            msg: binary_msg,
            funds: vec![],
            code_hash: gov_token_info.code_hash.clone(),
        }
        .into()],
    );

    // assert that the initial "threshold" address balance is 0
    let balance = query_balance_cw20(
        &app,
        gov_token_info.addr.clone().to_string(),
        gov_token_info.code_hash.clone(),
        "threshold",
        viewing_key_token.clone(),
    );
    assert_eq!(balance, Uint128::zero());

    // vote enough to pass the proposal
    vote_on_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        "threshold",
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key_threshold.clone(),
            address: "threshold".into(),
        },
        Vote::Yes,
    );

    // assert proposal is passed with 20 votes in favor and none opposed
    let proposal = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Passed);
    assert_eq!(proposal.proposal.votes.yes, Uint128::new(20));
    assert_eq!(proposal.proposal.votes.no, Uint128::zero());

    app.update_block(next_block);

    // the other voters wake up, vote against the proposal
    vote_on_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::No,
    );
    vote_on_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        "overslept_vote",
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key_overslept_vote.clone(),
            address: "overslept_vote".into(),
        },
        Vote::No,
    );

    app.update_block(next_block);

    // assert that the late votes have been counted and proposal
    // is still in passed state before executing it
    let proposal = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Passed);
    assert_eq!(proposal.proposal.votes.yes, Uint128::new(20));
    assert_eq!(proposal.proposal.votes.no, Uint128::new(80));

    execute_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );

    app.update_block(next_block);

    // make sure that the initial "threshold" address balance is
    // 100_000_000 and late votes did not make a difference
    let proposal = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Executed);
    assert_eq!(proposal.proposal.votes.yes, Uint128::new(20));
    assert_eq!(proposal.proposal.votes.no, Uint128::new(80));
    let balance = query_balance_cw20(
        &app,
        gov_token_info.addr.to_string(),
        gov_token_info.code_hash,
        "threshold",
        viewing_key_token,
    );
    assert_eq!(balance, Uint128::new(100_000_000));
}
