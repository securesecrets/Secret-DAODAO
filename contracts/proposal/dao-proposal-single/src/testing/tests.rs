use std::ops::Add;

use cosmwasm_std::{
    coins,
    testing::{mock_dependencies, mock_env, mock_info},
    to_binary, Addr, BankMsg, Binary, ContractInfo, ContractInfoResponse, CosmosMsg, Decimal,
    Uint128, WasmMsg, WasmQuery,
};
use cw_denom::CheckedDenom;
use cw_hooks::{HookError, HookItem, HooksResponse};
use dao_interface::{msg::InitialBalance, state::AnyContractInfo, voting::InfoResponse};
use dao_testing::{ShouldExecute, TestSingleChoiceVote};
use dao_voting::{
    deposit::{CheckedDepositInfo, UncheckedDepositInfo},
    pre_propose::{PreProposeInfo, ProposalCreationPolicy},
    proposal::{SingleChoiceProposeMsg as ProposeMsg, MAX_PROPOSAL_SIZE},
    status::Status,
    threshold::{ActiveThreshold, PercentageThreshold, Threshold},
    veto::{VetoConfig, VetoError},
    voting::{Vote, Votes},
};
use secret_cw2::ContractVersion;
use secret_multi_test::{next_block, App, Executor};
use secret_utils::Duration;
use shade_protocol::{basic_staking::Auth, Contract};

use crate::{
    contract::{migrate, CONTRACT_NAME, CONTRACT_VERSION},
    msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg},
    proposal::SingleChoiceProposal,
    query::ProposalResponse,
    state::Config,
    testing::{
        execute::{
            add_proposal_hook, add_proposal_hook_should_fail, add_vote_hook,
            add_vote_hook_should_fail, close_proposal, close_proposal_should_fail,
            create_snip20_viewing_key, execute_proposal, execute_proposal_should_fail,
            instantiate_sni20_base_default, make_proposal, mint_natives, mint_snip20s,
            remove_proposal_hook, remove_proposal_hook_should_fail, remove_vote_hook,
            remove_vote_hook_should_fail, update_rationale, vote_on_proposal,
            vote_on_proposal_should_fail,
        },
        instantiate::{
            get_default_non_token_dao_proposal_module_instantiate,
            get_default_token_dao_proposal_module_instantiate, get_pre_propose_info,
            instantiate_with_cw4_groups_governance, instantiate_with_staked_balances_governance,
            instantiate_with_staking_active_threshold,
        },
        queries::{
            query_balance_cw20, query_balance_native, query_creation_policy, query_dao_token,
            query_deposit_config_and_pre_propose_module, query_list_proposals,
            query_list_proposals_reverse, query_pre_proposal_single_deposit_info, query_proposal,
            query_proposal_config, query_proposal_hooks, query_single_proposal_module,
            query_vote_hooks, query_voting_module,
        },
    },
    ContractError,
};

use super::{
    do_votes::do_votes_staked_balances,
    execute::{create_viewing_key, vote_on_proposal_with_rationale},
    queries::{query_next_proposal_id, query_vote},
    CREATOR_ADDR,
};

struct CommonTest {
    app: App,
    core_contract_info: ContractInfo,
    proposal_module: AnyContractInfo,
    gov_token_info: AnyContractInfo,
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
        core_contract_info,
        proposal_module,
        gov_token_info,
        proposal_id,
        query_auth_info,
    }
}

#[test]
fn test_simple_propose_staked_balances() {
    let CommonTest {
        app,
        core_contract_info: _,
        proposal_module,
        gov_token_info,
        proposal_id,
        query_auth_info: _,
    } = setup_test(vec![]);

    let created = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    let current_block = app.block_info();

    // These values just come from the default instantiate message
    // values.
    let expected = SingleChoiceProposal {
        title: "title".to_string(),
        description: "description".to_string(),
        proposer: Addr::unchecked(CREATOR_ADDR),
        start_height: current_block.height,
        expiration: Duration::Time(604800).after(&current_block),
        min_voting_period: None,
        threshold: Threshold::ThresholdQuorum {
            quorum: PercentageThreshold::Percent(Decimal::percent(15)),
            threshold: PercentageThreshold::Majority {},
        },
        allow_revoting: false,
        total_power: Uint128::new(100_000_000),
        msgs: vec![],
        status: Status::Open,
        veto: None,
        votes: Votes::zero(),
    };

    assert_eq!(created.proposal, expected);
    assert_eq!(created.id, 1u64);

    // Check that the deposit info for this proposal looks right.
    let (_, pre_propose) = query_deposit_config_and_pre_propose_module(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
    );
    let deposit_response = query_pre_proposal_single_deposit_info(
        &app,
        &pre_propose.address.clone(),
        pre_propose.code_hash.clone(),
        proposal_id,
    );

    assert_eq!(deposit_response.proposer, Addr::unchecked(CREATOR_ADDR));
    assert_eq!(
        deposit_response.deposit_info,
        Some(CheckedDepositInfo {
            denom: cw_denom::CheckedDenom::Snip20(
                gov_token_info.addr.clone(),
                gov_token_info.code_hash.clone()
            ),
            amount: Uint128::new(10_000_000),
            refund_policy: dao_voting::deposit::DepositRefundPolicy::OnlyPassed
        })
    );
}

#[test]
fn test_simple_proposal_cw4_voting() {
    let mut app = App::default();
    let instantiate = get_default_non_token_dao_proposal_module_instantiate(&mut app);
    let core_contract_info = instantiate_with_cw4_groups_governance(&mut app, instantiate, None);
    let proposal_module = query_single_proposal_module(
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
    let id = make_proposal(
        &mut app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key,
            address: CREATOR_ADDR.into(),
        },
        vec![],
    );

    let created = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        id,
    );
    let current_block = app.block_info();

    // These values just come from the default instantiate message
    // values.
    let expected = SingleChoiceProposal {
        title: "title".to_string(),
        description: "description".to_string(),
        proposer: Addr::unchecked(CREATOR_ADDR),
        start_height: current_block.height,
        expiration: Duration::Time(604800).after(&current_block),
        min_voting_period: None,
        threshold: Threshold::ThresholdQuorum {
            threshold: PercentageThreshold::Percent(Decimal::percent(15)),
            quorum: PercentageThreshold::Majority {},
        },
        allow_revoting: false,
        total_power: Uint128::new(1),
        msgs: vec![],
        status: Status::Open,
        veto: None,
        votes: Votes::zero(),
    };

    assert_eq!(created.proposal, expected);
    assert_eq!(created.id, 1u64);

    // Check that the deposit info for this proposal looks right.
    let (_, pre_propose) = query_deposit_config_and_pre_propose_module(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
    );
    let deposit_response = query_pre_proposal_single_deposit_info(
        &app,
        &pre_propose.address,
        pre_propose.code_hash,
        id,
    );

    assert_eq!(deposit_response.proposer, Addr::unchecked(CREATOR_ADDR));
    assert_eq!(deposit_response.deposit_info, None,);
}

#[test]
fn test_propose_supports_stargate_messages() {
    // If we can make a proposal with a stargate message, we support
    // stargate messages in proposals.
    setup_test(vec![CosmosMsg::Stargate {
        type_url: "foo_type".to_string(),
        value: Binary::default(),
    }]);
}

/// Test that the deposit token is properly set to the voting module
/// token during instantiation.
#[test]
fn test_voting_module_token_instantiate() {
    let CommonTest {
        app,
        core_contract_info: _,
        proposal_module,
        gov_token_info,
        proposal_id,
        query_auth_info: _,
    } = setup_test(vec![]);

    let (_, pre_propose) = query_deposit_config_and_pre_propose_module(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
    );
    let deposit_response = query_pre_proposal_single_deposit_info(
        &app,
        &pre_propose.address,
        pre_propose.code_hash,
        proposal_id,
    );

    let deposit_token = if let Some(CheckedDepositInfo {
        denom: CheckedDenom::Snip20(addr, ..),
        ..
    }) = deposit_response.deposit_info
    {
        addr
    } else {
        panic!("voting module should have governance token")
    };
    assert_eq!(deposit_token, gov_token_info.addr.clone())
}

#[test]
#[should_panic(
    expected = "Error parsing into type dao_voting_cw4::msg::QueryMsg: unknown variant `token_contract`"
)]
fn test_deposit_token_voting_module_token_fails_if_no_voting_module_token() {
    let mut app = App::default();
    let instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate_with_cw4_groups_governance(&mut app, instantiate, None);
}

#[test]
fn test_instantiate_with_non_voting_module_cw20_deposit() {
    let mut app = App::default();
    let alt_snip20 = instantiate_sni20_base_default(&mut app);

    let mut instantiate = get_default_non_token_dao_proposal_module_instantiate(&mut app);
    // hehehehehehehehe
    instantiate.pre_propose_info = get_pre_propose_info(
        &mut app,
        Some(UncheckedDepositInfo {
            denom: dao_voting::deposit::DepositToken::Token {
                denom: cw_denom::UncheckedDenom::Snip20(
                    alt_snip20.address.clone().to_string(),
                    alt_snip20.code_hash.clone(),
                ),
            },
            amount: Uint128::new(10_000_000),
            refund_policy: dao_voting::deposit::DepositRefundPolicy::OnlyPassed,
        }),
        false,
    );

    let core_contract_info = instantiate_with_cw4_groups_governance(&mut app, instantiate, None);
    let proposal_module = query_single_proposal_module(
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

    let created = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    let current_block = app.block_info();

    // These values just come from the default instantiate message
    // values.
    let expected = SingleChoiceProposal {
        title: "title".to_string(),
        description: "description".to_string(),
        proposer: Addr::unchecked(CREATOR_ADDR),
        start_height: current_block.height,
        expiration: Duration::Time(604800).after(&current_block),
        min_voting_period: None,
        threshold: Threshold::ThresholdQuorum {
            threshold: PercentageThreshold::Percent(Decimal::percent(15)),
            quorum: PercentageThreshold::Majority {},
        },
        allow_revoting: false,
        total_power: Uint128::new(1),
        msgs: vec![],
        status: Status::Open,
        votes: Votes::zero(),
        veto: None,
    };

    assert_eq!(created.proposal, expected);
    assert_eq!(created.id, 1u64);

    // Check that the deposit info for this proposal looks right.
    let (_, pre_propose) = query_deposit_config_and_pre_propose_module(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
    );
    let deposit_response = query_pre_proposal_single_deposit_info(
        &app,
        &pre_propose.address,
        pre_propose.code_hash,
        proposal_id,
    );

    assert_eq!(deposit_response.proposer, Addr::unchecked(CREATOR_ADDR));
    assert_eq!(
        deposit_response.deposit_info,
        Some(CheckedDepositInfo {
            denom: cw_denom::CheckedDenom::Snip20(
                alt_snip20.address.clone(),
                alt_snip20.code_hash.clone()
            ),
            amount: Uint128::new(10_000_000),
            refund_policy: dao_voting::deposit::DepositRefundPolicy::OnlyPassed
        })
    );
}

#[test]
fn test_proposal_message_execution() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
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

    let viewing_key_token = create_snip20_viewing_key(
        &mut app,
        ContractInfo {
            address: gov_token_info.addr.clone(),
            code_hash: gov_token_info.code_hash.clone(),
        },
        mock_info(CREATOR_ADDR, &[]),
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
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );
    let snip20_balance = query_balance_cw20(
        &app,
        &gov_token_info.addr.clone(),
        gov_token_info.code_hash.clone(),
        CREATOR_ADDR,
        viewing_key_token.clone(),
    );
    let native_balance = query_balance_native(&app, CREATOR_ADDR, "ujuno");
    assert_eq!(snip20_balance, Uint128::zero());
    assert_eq!(native_balance, Uint128::zero());

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
        Vote::Yes,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Passed);

    // Can't use library function because we expect this to fail due
    // to insufficent balance in the bank module.
    app.execute_contract(
        Addr::unchecked(CREATOR_ADDR),
        &ContractInfo {
            address: proposal_module.addr.clone(),
            code_hash: proposal_module.code_hash.clone(),
        },
        &ExecuteMsg::Execute {
            auth: shade_protocol::basic_staking::Auth::ViewingKey {
                key: viewing_key.clone(),
                address: CREATOR_ADDR.into(),
            },
            proposal_id,
        },
        &[],
    )
    .unwrap_err();
    let proposal = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Passed);

    mint_natives(
        &mut app,
        core_contract_info.address.clone().as_str(),
        coins(10, "ujuno"),
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
    let proposal = query_proposal(
        &app,
        &proposal_module.addr.clone(),
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Executed);

    let snip20_balance = query_balance_cw20(
        &app,
        &gov_token_info.addr.clone(),
        gov_token_info.code_hash.clone(),
        CREATOR_ADDR,
        viewing_key_token,
    );
    let native_balance = query_balance_native(&app, CREATOR_ADDR, "ujuno");
    assert_eq!(snip20_balance, Uint128::new(20_000_000));
    assert_eq!(native_balance, Uint128::new(10));

    // Sneak in a check here that proposals can't be executed more
    // than once in the on close on execute config suituation.
    let err = execute_proposal_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash,
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );
    assert!(matches!(err, ContractError::NotPassed {}))
}

#[test]
fn test_proposal_message_timelock_execution() -> anyhow::Result<()> {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    let veto_config = VetoConfig {
        timelock_duration: Duration::Time(100),
        vetoer: "oversight".to_string(),
        early_execute: false,
        veto_before_passed: false,
    };
    instantiate.close_proposal_on_execution_failure = false;
    instantiate.veto = Some(veto_config.clone());
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![
            InitialBalance {
                address: "oversight".to_string(),
                amount: Uint128::new(15),
            },
            InitialBalance {
                address: CREATOR_ADDR.to_string(),
                amount: Uint128::new(85),
            },
        ]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
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

    let viewing_key_oversight = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("oversight", &[]),
    );

    let viewing_key_token = create_snip20_viewing_key(
        &mut app,
        ContractInfo {
            address: gov_token_info.addr.clone(),
            code_hash: gov_token_info.code_hash.clone(),
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );
    let snip20_balance = query_balance_cw20(
        &app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        CREATOR_ADDR,
        viewing_key_token.clone(),
    );
    let native_balance = query_balance_native(&app, CREATOR_ADDR, "ujuno");
    assert_eq!(snip20_balance, Uint128::zero());
    assert_eq!(native_balance, Uint128::zero());

    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );

    // Proposal is timelocked to the moment of prop expiring + timelock delay
    assert_eq!(
        proposal.proposal.status,
        Status::VetoTimelock {
            expiration: proposal
                .proposal
                .expiration
                .add(veto_config.timelock_duration)?,
        }
    );

    mint_natives(
        &mut app,
        core_contract_info.address.clone().as_str(),
        coins(10, "ujuno"),
    );

    // vetoer can't execute when timelock is active and
    // early execute not enabled.
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("oversight"),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Execute {
                auth: shade_protocol::basic_staking::Auth::ViewingKey {
                    key: viewing_key_oversight.clone(),
                    address: "oversight".into(),
                },
                proposal_id,
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::VetoError(VetoError::NoEarlyExecute {}));

    // Proposal cannot be excuted before timelock expires
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(CREATOR_ADDR),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Execute {
                auth: shade_protocol::basic_staking::Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: CREATOR_ADDR.into(),
                },
                proposal_id,
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    assert_eq!(err, ContractError::VetoError(VetoError::Timelocked {}));

    // Time passes
    app.update_block(|block| {
        block.time = block.time.plus_seconds(604800 + 200);
    });

    // Proposal executes successfully
    execute_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash,
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Executed);

    Ok(())
}

// only the authorized vetoer can veto an open proposal
#[test]
fn test_open_proposal_veto_unauthorized() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
    let veto_config = VetoConfig {
        timelock_duration: Duration::Time(100),
        vetoer: "oversight".to_string(),
        early_execute: false,
        veto_before_passed: true,
    };
    instantiate.veto = Some(veto_config.clone());
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(85),
        }]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key,
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );

    // only the vetoer can veto
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("not-oversight"),
            &ContractInfo {
                address: proposal_module.addr,
                code_hash: proposal_module.code_hash,
            },
            &ExecuteMsg::Veto { proposal_id },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::VetoError(VetoError::Unauthorized {}));
}

// open proposal can only be vetoed if `veto_before_passed` flag is enabled
#[test]
fn test_open_proposal_veto_with_early_veto_flag_disabled() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
    let veto_config = VetoConfig {
        timelock_duration: Duration::Time(100),
        vetoer: "oversight".to_string(),
        early_execute: false,
        veto_before_passed: false,
    };
    instantiate.veto = Some(veto_config.clone());
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(85),
        }]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("oversight"),
            &ContractInfo {
                address: proposal_module.addr,
                code_hash: proposal_module.code_hash,
            },
            &ExecuteMsg::Veto { proposal_id },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(
        err,
        ContractError::VetoError(VetoError::NoVetoBeforePassed {})
    );
}

#[test]
fn test_open_proposal_veto_with_no_timelock() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
    instantiate.veto = None;
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(85),
        }]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("oversight"),
            &ContractInfo {
                address: proposal_module.addr,
                code_hash: proposal_module.code_hash,
            },
            &ExecuteMsg::Veto { proposal_id },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(
        err,
        ContractError::VetoError(VetoError::NoVetoConfiguration {})
    );
}

// if proposal is not open or timelocked, attempts to veto should
// throw an error
#[test]
fn test_vetoed_proposal_veto() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
    let veto_config = VetoConfig {
        timelock_duration: Duration::Time(100),
        vetoer: "oversight".to_string(),
        early_execute: false,
        veto_before_passed: true,
    };
    instantiate.veto = Some(veto_config.clone());
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(85),
        }]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );

    app.execute_contract(
        Addr::unchecked("oversight"),
        &ContractInfo {
            address: proposal_module.addr.clone(),
            code_hash: proposal_module.code_hash.clone(),
        },
        &ExecuteMsg::Veto { proposal_id },
        &[],
    )
    .unwrap();

    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Vetoed {});

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("oversight"),
            &ContractInfo {
                address: proposal_module.addr,
                code_hash: proposal_module.code_hash,
            },
            &ExecuteMsg::Veto { proposal_id },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    assert_eq!(
        ContractError::VetoError(VetoError::InvalidProposalStatus {
            status: "vetoed".to_string()
        }),
        err,
    );
}

#[test]
fn test_open_proposal_veto_early() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
    let veto_config = VetoConfig {
        timelock_duration: Duration::Time(100),
        vetoer: "oversight".to_string(),
        early_execute: false,
        veto_before_passed: true,
    };
    instantiate.veto = Some(veto_config.clone());
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(85),
        }]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );

    app.execute_contract(
        Addr::unchecked("oversight"),
        &ContractInfo {
            address: proposal_module.addr.clone(),
            code_hash: proposal_module.code_hash.clone(),
        },
        &ExecuteMsg::Veto { proposal_id },
        &[],
    )
    .unwrap();

    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash,
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Vetoed {});
}

// only the vetoer can veto during timelock period
#[test]
fn test_timelocked_proposal_veto_unauthorized() -> anyhow::Result<()> {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
    let veto_config = VetoConfig {
        timelock_duration: Duration::Time(100),
        vetoer: "oversight".to_string(),
        early_execute: true,
        veto_before_passed: false,
    };
    instantiate.veto = Some(veto_config.clone());
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![
            InitialBalance {
                address: "oversight".to_string(),
                amount: Uint128::new(15),
            },
            InitialBalance {
                address: CREATOR_ADDR.to_string(),
                amount: Uint128::new(85),
            },
        ]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );

    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );

    // Proposal is timelocked to the moment of prop expiring + timelock delay
    assert_eq!(
        proposal.proposal.status,
        Status::VetoTimelock {
            expiration: proposal
                .proposal
                .expiration
                .add(veto_config.timelock_duration)?,
        }
    );

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("not-oversight"),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Veto { proposal_id },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    assert_eq!(err, ContractError::VetoError(VetoError::Unauthorized {}),);
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash,
        proposal_id,
    );
    assert_eq!(
        proposal.proposal.status,
        Status::VetoTimelock {
            expiration: proposal
                .proposal
                .expiration
                .add(veto_config.timelock_duration)?,
        }
    );

    Ok(())
}

// vetoer can only veto the proposal before the timelock expires
#[test]
fn test_timelocked_proposal_veto_expired_timelock() -> anyhow::Result<()> {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
    let veto_config = VetoConfig {
        timelock_duration: Duration::Time(100),
        vetoer: "oversight".to_string(),
        early_execute: true,
        veto_before_passed: false,
    };
    instantiate.veto = Some(veto_config.clone());
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![
            InitialBalance {
                address: "oversight".to_string(),
                amount: Uint128::new(15),
            },
            InitialBalance {
                address: CREATOR_ADDR.to_string(),
                amount: Uint128::new(85),
            },
        ]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );

    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );

    // Proposal is timelocked to the moment of prop expiring + timelock delay
    assert_eq!(
        proposal.proposal.status,
        Status::VetoTimelock {
            expiration: proposal
                .proposal
                .expiration
                .add(veto_config.timelock_duration)?,
        }
    );
    app.update_block(|b| b.time = b.time.plus_seconds(604800 + 200));

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("oversight"),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Veto { proposal_id },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    assert_eq!(err, ContractError::VetoError(VetoError::TimelockExpired {}),);

    Ok(())
}

// vetoer can only exec timelocked prop if the early exec flag is enabled
#[test]
fn test_timelocked_proposal_execute_no_early_exec() -> anyhow::Result<()> {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
    let veto_config = VetoConfig {
        timelock_duration: Duration::Time(100),
        vetoer: "oversight".to_string(),
        early_execute: false,
        veto_before_passed: false,
    };
    instantiate.veto = Some(veto_config.clone());
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(85),
        }]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );

    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );

    // Proposal is timelocked to the moment of prop expiring + timelock delay
    assert_eq!(
        proposal.proposal.status,
        Status::VetoTimelock {
            expiration: proposal
                .proposal
                .expiration
                .add(veto_config.timelock_duration)?,
        }
    );

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("oversight"),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Execute {
                auth: shade_protocol::basic_staking::Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: CREATOR_ADDR.into(),
                },
                proposal_id,
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    assert_eq!(err, ContractError::VetoError(VetoError::NoEarlyExecute {}),);

    Ok(())
}

#[test]
fn test_timelocked_proposal_execute_early() -> anyhow::Result<()> {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
    let veto_config = VetoConfig {
        timelock_duration: Duration::Time(100),
        vetoer: "oversight".to_string(),
        early_execute: true,
        veto_before_passed: false,
    };
    instantiate.veto = Some(veto_config.clone());
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(85),
        }]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );

    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    // Proposal is timelocked to the moment of prop expiring + timelock delay
    assert_eq!(
        proposal.proposal.status,
        Status::VetoTimelock {
            expiration: proposal
                .proposal
                .expiration
                .add(veto_config.timelock_duration)?,
        }
    );

    // assert timelock is active
    assert!(!veto_config
        .timelock_duration
        .after(&app.block_info())
        .is_expired(&app.block_info()));
    mint_natives(
        &mut app,
        core_contract_info.address.clone().as_str(),
        coins(10, "ujuno"),
    );

    app.execute_contract(
        Addr::unchecked("oversight"),
        &ContractInfo {
            address: proposal_module.addr.clone(),
            code_hash: proposal_module.code_hash.clone(),
        },
        &ExecuteMsg::Execute {
            auth: shade_protocol::basic_staking::Auth::ViewingKey {
                key: viewing_key,
                address: CREATOR_ADDR.into(),
            },
            proposal_id,
        },
        &[],
    )
    .unwrap();

    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash,
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Executed {});

    Ok(())
}

// only vetoer can exec timelocked prop early
#[test]
fn test_timelocked_proposal_execute_active_timelock_unauthorized() -> anyhow::Result<()> {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
    let veto_config = VetoConfig {
        timelock_duration: Duration::Time(100),
        vetoer: "oversight".to_string(),
        early_execute: true,
        veto_before_passed: false,
    };
    instantiate.veto = Some(veto_config.clone());
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(85),
        }]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );

    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    // Proposal is timelocked to the moment of prop expiring + timelock delay
    assert_eq!(
        proposal.proposal.status,
        Status::VetoTimelock {
            expiration: proposal
                .proposal
                .expiration
                .add(veto_config.timelock_duration)?,
        }
    );

    // assert timelock is active
    assert!(!veto_config
        .timelock_duration
        .after(&app.block_info())
        .is_expired(&app.block_info()));

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(CREATOR_ADDR),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Execute {
                auth: shade_protocol::basic_staking::Auth::ViewingKey {
                    key: viewing_key,
                    address: CREATOR_ADDR.into(),
                },
                proposal_id,
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    assert_eq!(err, ContractError::VetoError(VetoError::Timelocked {}),);

    Ok(())
}

// anyone can exec the prop after the timelock expires
#[test]
fn test_timelocked_proposal_execute_expired_timelock_not_vetoer() -> anyhow::Result<()> {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
    let veto_config = VetoConfig {
        timelock_duration: Duration::Time(100),
        vetoer: "oversight".to_string(),
        early_execute: true,
        veto_before_passed: false,
    };
    instantiate.veto = Some(veto_config.clone());
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(85),
        }]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );

    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );

    // Proposal is timelocked to the moment of prop expiring + timelock delay
    let expiration = proposal
        .proposal
        .expiration
        .add(veto_config.timelock_duration)?;
    assert_eq!(
        proposal.proposal.status,
        Status::VetoTimelock { expiration }
    );

    app.update_block(|b| b.time = b.time.plus_seconds(604800 + 201));
    // assert timelock is expired
    assert!(expiration.is_expired(&app.block_info()));
    mint_natives(
        &mut app,
        core_contract_info.address.clone().as_str(),
        coins(10, "ujuno"),
    );

    app.execute_contract(
        Addr::unchecked(CREATOR_ADDR),
        &ContractInfo {
            address: proposal_module.addr.clone(),
            code_hash: proposal_module.code_hash.clone(),
        },
        &ExecuteMsg::Execute {
            auth: shade_protocol::basic_staking::Auth::ViewingKey {
                key: viewing_key,
                address: CREATOR_ADDR.into(),
            },
            proposal_id,
        },
        &[],
    )
    .unwrap();

    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash,
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Executed {},);

    Ok(())
}

#[test]
fn test_proposal_message_timelock_veto() -> anyhow::Result<()> {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
    let veto_config = VetoConfig {
        timelock_duration: Duration::Time(100),
        vetoer: "oversight".to_string(),
        early_execute: false,
        veto_before_passed: false,
    };
    instantiate.veto = Some(veto_config.clone());
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(85),
        }]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
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

    let viewing_key_token = create_snip20_viewing_key(
        &mut app,
        ContractInfo {
            address: gov_token_info.addr.clone(),
            code_hash: gov_token_info.code_hash.clone(),
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );
    let snip20_balance = query_balance_cw20(
        &app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        CREATOR_ADDR,
        viewing_key_token.clone(),
    );
    let native_balance = query_balance_native(&app, CREATOR_ADDR, "ujuno");
    assert_eq!(snip20_balance, Uint128::zero());
    assert_eq!(native_balance, Uint128::zero());

    // Vetoer can't veto early
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("oversight"),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Veto { proposal_id },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(
        err,
        ContractError::VetoError(VetoError::NoVetoBeforePassed {})
    );

    // Vote on proposal to pass it
    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    // Proposal is timelocked to the moment of prop expiring + timelock delay
    assert_eq!(
        proposal.proposal.status,
        Status::VetoTimelock {
            expiration: proposal
                .proposal
                .expiration
                .add(veto_config.timelock_duration)?,
        }
    );

    mint_natives(
        &mut app,
        core_contract_info.address.clone().as_str(),
        coins(10, "ujuno"),
    );

    // Non-vetoer cannot veto
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(CREATOR_ADDR),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Veto { proposal_id },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::VetoError(VetoError::Unauthorized {}));

    // Oversite vetos prop
    app.execute_contract(
        Addr::unchecked("oversight"),
        &ContractInfo {
            address: proposal_module.addr.clone(),
            code_hash: proposal_module.code_hash.clone(),
        },
        &ExecuteMsg::Veto { proposal_id },
        &[],
    )
    .unwrap();

    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Vetoed);

    Ok(())
}

#[test]
fn test_proposal_message_timelock_early_execution() -> anyhow::Result<()> {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
    let veto_config = VetoConfig {
        timelock_duration: Duration::Time(100),
        vetoer: "oversight".to_string(),
        early_execute: true,
        veto_before_passed: false,
    };
    instantiate.veto = Some(veto_config.clone());
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![
            InitialBalance {
                address: "oversight".to_string(),
                amount: Uint128::new(15),
            },
            InitialBalance {
                address: CREATOR_ADDR.to_string(),
                amount: Uint128::new(85),
            },
        ]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
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

    let viewing_key_token = create_snip20_viewing_key(
        &mut app,
        ContractInfo {
            address: gov_token_info.addr.clone(),
            code_hash: gov_token_info.code_hash.clone(),
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    let viewing_key_oversight = create_snip20_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("oversight", &[]),
    );

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );
    let snip20_balance = query_balance_cw20(
        &app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        CREATOR_ADDR,
        viewing_key_token.clone(),
    );
    let native_balance = query_balance_native(&app, CREATOR_ADDR, "ujuno");
    assert_eq!(snip20_balance, Uint128::zero());
    assert_eq!(native_balance, Uint128::zero());

    // Vote on proposal to pass it
    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    // Proposal is timelocked to the moment of prop expiring + timelock delay
    assert_eq!(
        proposal.proposal.status,
        Status::VetoTimelock {
            expiration: proposal
                .proposal
                .expiration
                .add(veto_config.timelock_duration)?,
        }
    );

    mint_natives(
        &mut app,
        core_contract_info.address.clone().as_str(),
        coins(10, "ujuno"),
    );

    // Proposal can be executed early by vetoer
    execute_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        "oversight",
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key_oversight.clone(),
            address: "oversight".into(),
        },
        proposal_id,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash,
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Executed);

    Ok(())
}

#[test]
fn test_proposal_message_timelock_veto_before_passed() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
    instantiate.veto = Some(VetoConfig {
        timelock_duration: Duration::Time(100),
        vetoer: "oversight".to_string(),
        early_execute: false,
        veto_before_passed: true,
    });
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![
            InitialBalance {
                address: "oversight".to_string(),
                amount: Uint128::new(15),
            },
            InitialBalance {
                address: CREATOR_ADDR.to_string(),
                amount: Uint128::new(85),
            },
        ]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );

    // Proposal is open for voting
    assert_eq!(proposal.proposal.status, Status::Open);

    // Oversite vetos prop
    app.execute_contract(
        Addr::unchecked("oversight"),
        &ContractInfo {
            address: proposal_module.addr.clone(),
            code_hash: proposal_module.code_hash.clone(),
        },
        &ExecuteMsg::Veto { proposal_id },
        &[],
    )
    .unwrap();

    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash,
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Vetoed);

    // mint_natives(&mut app, core_contract_info.as_str(), coins(10, "ujuno"));

    // // Proposal can be executed early by vetoer
    // execute_proposal(&mut app, &proposal_module, "oversight", proposal_id);
    // let proposal = query_proposal(&app, &proposal_module, proposal_id);
    // assert_eq!(proposal.proposal.status, Status::Executed);
}

#[test]
fn test_veto_only_members_execute_proposal() -> anyhow::Result<()> {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.close_proposal_on_execution_failure = false;
    let veto_config = VetoConfig {
        timelock_duration: Duration::Time(100),
        vetoer: "oversight".to_string(),
        early_execute: true,
        veto_before_passed: false,
    };
    instantiate.veto = Some(veto_config.clone());
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::new(85),
        }]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
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

    let viewing_key_token = create_snip20_viewing_key(
        &mut app,
        ContractInfo {
            address: gov_token_info.addr.clone(),
            code_hash: gov_token_info.code_hash.clone(),
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    let viewing_key_oversight = create_snip20_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("oversight", &[]),
    );

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![
            WasmMsg::Execute {
                contract_addr: gov_token_info.addr.clone().to_string(),
                code_hash: gov_token_info.code_hash.clone(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Mint {
                    recipient: CREATOR_ADDR.to_string(),
                    amount: Uint128::new(10_000_000),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap(),
                funds: vec![],
            }
            .into(),
            BankMsg::Send {
                to_address: CREATOR_ADDR.to_string(),
                amount: coins(10, "ujuno"),
            }
            .into(),
        ],
    );
    let snip20_balance = query_balance_cw20(
        &app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        CREATOR_ADDR,
        viewing_key_token.clone(),
    );
    let native_balance = query_balance_native(&app, CREATOR_ADDR, "ujuno");
    assert_eq!(snip20_balance, Uint128::zero());
    assert_eq!(native_balance, Uint128::zero());

    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );

    // Proposal is timelocked to the moment of prop expiring + timelock delay
    let expiration = proposal
        .proposal
        .expiration
        .add(veto_config.timelock_duration)?;
    assert_eq!(
        proposal.proposal.status,
        Status::VetoTimelock { expiration }
    );

    app.update_block(|b| b.time = b.time.plus_seconds(604800 + 101));
    // assert timelock is expired
    assert!(expiration.is_expired(&app.block_info()));
    mint_natives(
        &mut app,
        core_contract_info.address.clone().as_str(),
        coins(10, "ujuno"),
    );

    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Passed);

    // Proposal cannot be executed by vetoer once timelock expired
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked("oversight"),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Execute {
                auth: shade_protocol::basic_staking::Auth::ViewingKey {
                    key: viewing_key_oversight,
                    address: "oversight".to_string(),
                },
                proposal_id,
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::Unauthorized {});

    // Proposal can be executed by member once timelock expired
    execute_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key,
            address: CREATOR_ADDR.to_string(),
        },
        proposal_id,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash,
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Executed);

    Ok(())
}

#[test]
fn test_proposal_close_after_expiry() {
    let CommonTest {
        mut app,
        core_contract_info,
        proposal_module,
        gov_token_info: _,
        proposal_id,
        query_auth_info: _,
    } = setup_test(vec![BankMsg::Send {
        to_address: CREATOR_ADDR.to_string(),
        amount: coins(10, "ujuno"),
    }
    .into()]);
    mint_natives(
        &mut app,
        core_contract_info.address.as_str(),
        coins(10, "ujuno"),
    );

    // Try and close the proposal. This shoudl fail as the proposal is
    // open.
    let err = close_proposal_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
    );
    assert!(matches!(err, ContractError::WrongCloseStatus {}));

    // Expire the proposal. Now it should be closable.
    app.update_block(|b| b.time = b.time.plus_seconds(604800));
    close_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash,
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Closed);
}

#[test]
fn test_proposal_cant_close_after_expiry_is_passed() {
    let mut app = App::default();
    let instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![
            InitialBalance {
                address: "quorum".to_string(),
                amount: Uint128::new(15),
            },
            InitialBalance {
                address: CREATOR_ADDR.to_string(),
                amount: Uint128::new(85),
            },
        ]),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    mint_natives(
        &mut app,
        core_contract_info.address.clone().as_str(),
        coins(10, "ujuno"),
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

    let viewing_key_quorum = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("quorum", &[]),
    );

    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![BankMsg::Send {
            to_address: CREATOR_ADDR.to_string(),
            amount: coins(10, "ujuno"),
        }
        .into()],
    );
    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        "quorum",
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key_quorum.clone(),
            address: "quorum".into(),
        },
        Vote::Yes,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Open);

    // Expire the proposal. This should pass it.
    app.update_block(|b| b.time = b.time.plus_seconds(604800));
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Passed,);

    // Make sure it can't be closed.
    let err = close_proposal_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
    );
    assert!(matches!(err, ContractError::WrongCloseStatus {}));

    // Executed proposals may not be closed.
    execute_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );
    let err = close_proposal_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
    );
    assert!(matches!(err, ContractError::WrongCloseStatus {}));
    let balance = query_balance_native(&app, CREATOR_ADDR, "ujuno");
    assert_eq!(balance, Uint128::new(10));
    let err = close_proposal_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
    );
    assert!(matches!(err, ContractError::WrongCloseStatus {}));
}

#[test]
fn test_execute_no_non_passed_execution() {
    let CommonTest {
        mut app,
        core_contract_info,
        proposal_module,
        gov_token_info,
        proposal_id,
        query_auth_info,
    } = setup_test(vec![BankMsg::Send {
        to_address: CREATOR_ADDR.to_string(),
        amount: coins(10, "ujuno"),
    }
    .into()]);
    mint_natives(
        &mut app,
        core_contract_info.address.clone().as_str(),
        coins(100, "ujuno"),
    );

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    let err = execute_proposal_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );
    assert!(matches!(err, ContractError::NotPassed {}));

    // Expire the proposal.
    app.update_block(|b| b.time = b.time.plus_seconds(604800));
    let err = execute_proposal_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );
    assert!(matches!(err, ContractError::NotPassed {}));

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![],
    );
    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    execute_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );
    // Can't execute more than once.
    let err = execute_proposal_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash,
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );
    assert!(matches!(err, ContractError::NotPassed {}));
}

#[test]
fn test_cant_execute_not_member_when_proposal_created() {
    let CommonTest {
        mut app,
        core_contract_info,
        proposal_module,
        gov_token_info,
        proposal_id,
        query_auth_info,
    } = setup_test(vec![BankMsg::Send {
        to_address: CREATOR_ADDR.to_string(),
        amount: coins(10, "ujuno"),
    }
    .into()]);
    mint_natives(
        &mut app,
        core_contract_info.address.clone().as_str(),
        coins(100, "ujuno"),
    );

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    let viewing_key_noah = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("noah", &[]),
    );

    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );

    // Give noah some tokens.
    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        "noah",
        20_000_000,
    );
    // Have noah stake some.
    let voting_module = query_voting_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let staking_contract: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_module.code_hash.clone(),
            voting_module.addr.clone(),
            &dao_voting_snip20_staked::msg::QueryMsg::StakingContract {},
        )
        .unwrap();
    app.execute_contract(
        Addr::unchecked("noah"),
        &ContractInfo {
            address: gov_token_info.addr.clone(),
            code_hash: gov_token_info.code_hash.clone(),
        },
        &snip20_reference_impl::msg::ExecuteMsg::Send {
            recipient: staking_contract.addr.clone().to_string(),
            recipient_code_hash: Some(staking_contract.code_hash.clone()),
            amount: Uint128::new(10_000_000),
            msg: Some(
                to_binary(&snip20_stake::msg::ReceiveMsg::Stake {
                    auth: Box::new(shade_protocol::basic_staking::Auth::ViewingKey {
                        key: viewing_key_noah.clone(),
                        address: "noah".into(),
                    }),
                })
                .unwrap(),
            ),
            memo: None,
            decoys: None,
            entropy: None,
            padding: None,
        },
        &[],
    )
    .unwrap();
    // Update the block so that the staked balance appears.
    app.update_block(|block| block.height += 1);

    // println!("here");

    // // Can't execute from member who wasn't a member when the proposal was
    // // created.
    // let err = execute_proposal_should_fail(
    //     &mut app,
    //     &proposal_module.addr,
    //     proposal_module.code_hash.clone(),
    //     "noah",
    //     shade_protocol::basic_staking::Auth::ViewingKey {
    //         key: viewing_key_noah.clone(),
    //         address: "noah".into(),
    //     },
    //     proposal_id,
    // );
    // assert!(matches!(err, ContractError::Unauthorized {}));
}

#[test]
fn test_update_config() {
    let CommonTest {
        mut app,
        core_contract_info,
        proposal_module,
        gov_token_info: _,
        proposal_id,
        query_auth_info,
    } = setup_test(vec![]);

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    execute_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );
    // Make a proposal to update the config.
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![WasmMsg::Execute {
            contract_addr: proposal_module.addr.clone().to_string(),
            code_hash: proposal_module.code_hash.clone(),
            msg: to_binary(&ExecuteMsg::UpdateConfig {
                veto: Some(VetoConfig {
                    timelock_duration: Duration::Height(2),
                    vetoer: CREATOR_ADDR.to_string(),
                    early_execute: false,
                    veto_before_passed: false,
                }),
                threshold: Threshold::AbsoluteCount {
                    threshold: Uint128::new(10_000),
                },
                max_voting_period: Duration::Height(6),
                min_voting_period: None,
                only_members_execute: true,
                allow_revoting: false,
                close_proposal_on_execution_failure: false,
            })
            .unwrap(),
            funds: vec![],
        }
        .into()],
    );
    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    execute_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );

    let config = query_proposal_config(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
    );
    assert_eq!(
        config,
        Config {
            veto: Some(VetoConfig {
                timelock_duration: Duration::Height(2),
                vetoer: CREATOR_ADDR.to_string(),
                early_execute: false,
                veto_before_passed: false,
            }),
            threshold: Threshold::AbsoluteCount {
                threshold: Uint128::new(10_000)
            },
            max_voting_period: Duration::Height(6),
            min_voting_period: None,
            only_members_execute: true,
            allow_revoting: false,
            close_proposal_on_execution_failure: false,
            query_auth: Contract {
                address: query_auth_info.addr.clone(),
                code_hash: query_auth_info.code_hash.clone()
            }
        }
    );

    // Check that non-dao address may not update config.
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(CREATOR_ADDR),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &&ExecuteMsg::UpdateConfig {
                veto: None,
                threshold: Threshold::AbsoluteCount {
                    threshold: Uint128::new(10_000),
                },
                max_voting_period: Duration::Height(6),
                min_voting_period: None,
                only_members_execute: true,
                allow_revoting: false,
                close_proposal_on_execution_failure: false,
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert!(matches!(err, ContractError::Unauthorized {}));

    // Check that veto config is validated (mismatching duration units).
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(core_contract_info.address.clone()),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &&ExecuteMsg::UpdateConfig {
                veto: Some(VetoConfig {
                    timelock_duration: Duration::Time(100),
                    vetoer: CREATOR_ADDR.to_string(),
                    early_execute: false,
                    veto_before_passed: false,
                }),
                threshold: Threshold::AbsoluteCount {
                    threshold: Uint128::new(10_000),
                },
                max_voting_period: Duration::Height(6),
                min_voting_period: None,
                only_members_execute: true,
                allow_revoting: false,
                close_proposal_on_execution_failure: false,
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert!(matches!(
        err,
        ContractError::VetoError(VetoError::TimelockDurationUnitMismatch {})
    ))
}

#[test]
fn test_anyone_may_propose_and_proposal_listing() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.pre_propose_info = PreProposeInfo::AnyoneMayPropose {};
    let core_contract_info =
        instantiate_with_staked_balances_governance(&mut app, instantiate, None);
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
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

    let viewing_key_creator = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info(CREATOR_ADDR, &[]),
    );
    for addr in 'm'..'z' {
        let addr = addr.to_string().repeat(6);
        let viewing_key = create_viewing_key(
            &mut app,
            ContractInfo {
                address: query_auth_info.addr.clone(),
                code_hash: query_auth_info.code_hash.clone(),
            },
            mock_info(&addr, &[]),
        );
        let proposal_id = make_proposal(
            &mut app,
            &proposal_module.addr,
            proposal_module.code_hash.clone(),
            &addr,
            Auth::ViewingKey {
                key: viewing_key.clone(),
                address: addr.clone(),
            },
            vec![],
        );
        vote_on_proposal(
            &mut app,
            &proposal_module.addr,
            proposal_module.code_hash.clone(),
            CREATOR_ADDR,
            proposal_id,
            Auth::ViewingKey {
                key: viewing_key_creator.clone(),
                address: CREATOR_ADDR.into(),
            },
            Vote::Yes,
        );

        // Only members can execute still.
        let err = execute_proposal_should_fail(
            &mut app,
            &proposal_module.addr,
            proposal_module.code_hash.clone(),
            &addr,
            Auth::ViewingKey {
                key: viewing_key.clone(),
                address: addr.clone(),
            },
            proposal_id,
        );
        assert!(matches!(err, ContractError::Unauthorized {}));
        execute_proposal(
            &mut app,
            &proposal_module.addr,
            proposal_module.code_hash.clone(),
            CREATOR_ADDR,
            Auth::ViewingKey {
                key: viewing_key_creator.clone(),
                address: CREATOR_ADDR.into(),
            },
            proposal_id,
        );
    }

    // Now that we've got all these proposals sitting around, lets
    // test that we can query them.

    let proposals_forward = query_list_proposals(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        None,
        None,
    );

    let mut proposals_reverse = query_list_proposals_reverse(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        None,
        None,
    );
    proposals_reverse.proposals.reverse();
    assert_eq!(proposals_reverse, proposals_forward);

    // Check the proposers and (implicitly) the ordering.
    for (index, addr) in ('m'..'z').enumerate() {
        let addr = addr.to_string().repeat(6);
        assert_eq!(
            proposals_forward.proposals[index].proposal.proposer,
            Addr::unchecked(addr)
        )
    }

    let four_and_five = query_list_proposals(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        Some(3),
        Some(2),
    );
    let mut five_and_four = query_list_proposals_reverse(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        Some(6),
        Some(2),
    );
    five_and_four.proposals.reverse();

    assert_eq!(five_and_four, four_and_five);
    assert_eq!(
        four_and_five.proposals[0].proposal.proposer,
        Addr::unchecked("pppppp")
    );

    let current_block = app.block_info();
    assert_eq!(
        four_and_five.proposals[0],
        ProposalResponse {
            id: 4,
            proposal: SingleChoiceProposal {
                title: "title".to_string(),
                description: "description".to_string(),
                proposer: Addr::unchecked("pppppp"),
                start_height: current_block.height,
                min_voting_period: None,
                expiration: Duration::Time(604800).after(&current_block),
                threshold: Threshold::ThresholdQuorum {
                    quorum: PercentageThreshold::Percent(Decimal::percent(15)),
                    threshold: PercentageThreshold::Majority {},
                },
                allow_revoting: false,
                total_power: Uint128::new(100_000_000),
                msgs: vec![],
                status: Status::Executed,
                votes: Votes {
                    yes: Uint128::new(100_000_000),
                    no: Uint128::zero(),
                    abstain: Uint128::zero()
                },
                veto: None
            }
        }
    )
}

#[test]
fn test_proposal_hook_registration() {
    let CommonTest {
        mut app,
        core_contract_info,
        proposal_module,
        gov_token_info: _,
        proposal_id: _,
        ..
    } = setup_test(vec![]);

    let proposal_hooks = query_proposal_hooks(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
    );
    assert_eq!(
        proposal_hooks.hooks.len(),
        0,
        "pre-propose deposit module should not show on this listing"
    );

    // non-dao may not add a hook.
    let err = add_proposal_hook_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        "proposalhook",
        "proposalhook_code_hash".to_string(),
    );
    assert!(matches!(err, ContractError::Unauthorized {}));

    add_proposal_hook(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        core_contract_info.address.clone().as_str(),
        "proposalhook",
        "proposalhook_code_hash".into(),
    );
    let err = add_proposal_hook_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        core_contract_info.address.clone().as_str(),
        "proposalhook",
        "proposalhook_code_hash".into(),
    );
    assert!(matches!(
        err,
        ContractError::HookError(HookError::HookAlreadyRegistered {})
    ));

    let proposal_hooks = query_proposal_hooks(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
    );
    assert_eq!(
        proposal_hooks.hooks[0],
        HookItem {
            addr: Addr::unchecked("proposalhook"),
            code_hash: "proposalhook_code_hash".into()
        }
    );

    // Only DAO can remove proposal hooks.
    let err = remove_proposal_hook_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        "proposalhook",
        "proposalhook_code_hash".into(),
    );
    assert!(matches!(err, ContractError::Unauthorized {}));
    remove_proposal_hook(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        core_contract_info.address.clone().as_str(),
        "proposalhook",
        "proposalhook_code_hash".into(),
    );
    let proposal_hooks = query_proposal_hooks(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
    );
    assert_eq!(proposal_hooks.hooks.len(), 0);

    // Can not remove that which does not exist.
    let err = remove_proposal_hook_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash,
        core_contract_info.address.as_str(),
        "proposalhook",
        "proposalhook_code_hash".into(),
    );
    assert!(matches!(
        err,
        ContractError::HookError(HookError::HookNotRegistered {})
    ));
}

#[test]
fn test_vote_hook_registration() {
    let CommonTest {
        mut app,
        core_contract_info,
        proposal_module,
        gov_token_info: _,
        proposal_id: _,
        ..
    } = setup_test(vec![]);

    let vote_hooks = query_vote_hooks(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
    );
    assert!(vote_hooks.hooks.is_empty(),);

    // non-dao may not add a hook.
    let err = add_vote_hook_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        "votehook",
        "votehook_codehash".into(),
    );
    assert!(matches!(err, ContractError::Unauthorized {}));

    add_vote_hook(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        core_contract_info.address.clone().as_str(),
        "votehook",
        "votehook_codehash".into(),
    );

    let vote_hooks = query_vote_hooks(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
    );
    assert_eq!(
        vote_hooks,
        HooksResponse {
            hooks: vec![HookItem {
                addr: Addr::unchecked("votehook"),
                code_hash: "votehook_codehash".into()
            }]
        }
    );

    let err = add_vote_hook_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        core_contract_info.address.clone().as_str(),
        "votehook",
        "votehook_codehash".into(),
    );
    assert!(matches!(
        err,
        ContractError::HookError(HookError::HookAlreadyRegistered {})
    ));

    let vote_hooks = query_vote_hooks(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
    );
    assert_eq!(
        vote_hooks.hooks[0],
        HookItem {
            addr: Addr::unchecked("votehook"),
            code_hash: "votehook_codehash".into()
        }
    );

    // Only DAO can remove vote hooks.
    let err = remove_vote_hook_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        "votehook",
        "votehook_codehash".into(),
    );
    assert!(matches!(err, ContractError::Unauthorized {}));
    remove_vote_hook(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        core_contract_info.address.as_str(),
        "votehook",
        "votehook_codehash".into(),
    );

    let vote_hooks = query_vote_hooks(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
    );
    assert!(vote_hooks.hooks.is_empty(),);

    // Can not remove that which does not exist.
    let err = remove_vote_hook_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash,
        core_contract_info.address.as_str(),
        "votehook",
        "votehook_codehash".into(),
    );
    assert!(matches!(
        err,
        ContractError::HookError(HookError::HookNotRegistered {})
    ));
}

#[test]
fn test_active_threshold_absolute() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.pre_propose_info = PreProposeInfo::AnyoneMayPropose {};
    let core_contract_info = instantiate_with_staking_active_threshold(
        &mut app,
        instantiate,
        None,
        Some(ActiveThreshold::AbsoluteCount {
            count: Uint128::new(100),
        }),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let voting_module = query_voting_module(
        &app,
        &core_contract_info.address,
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

    let staking_contract: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_module.code_hash.clone(),
            voting_module.addr.clone(),
            &dao_voting_snip20_staked::msg::QueryMsg::StakingContract {},
        )
        .unwrap();

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(CREATOR_ADDR),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Propose(ProposeMsg {
                title: "title".to_string(),
                description: "description".to_string(),
                msgs: vec![],
                proposer: None,
            }),
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert!(matches!(err, ContractError::InactiveDao {}));

    let msg = snip20_reference_impl::msg::ExecuteMsg::Send {
        recipient: staking_contract.addr.clone().to_string(),
        amount: Uint128::new(100),
        msg: Some(
            to_binary(&snip20_stake::msg::ReceiveMsg::Stake {
                auth: Box::new(Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: CREATOR_ADDR.into(),
                }),
            })
            .unwrap(),
        ),
        recipient_code_hash: Some(staking_contract.code_hash.clone()),
        memo: None,
        decoys: None,
        entropy: None,
        padding: None,
    };
    app.execute_contract(
        Addr::unchecked(CREATOR_ADDR),
        &ContractInfo {
            address: gov_token_info.addr.clone(),
            code_hash: gov_token_info.code_hash.clone(),
        },
        &msg,
        &[],
    )
    .unwrap();
    app.update_block(next_block);

    // Proposal creation now works as tokens have been staked to reach
    // active threshold.
    make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![],
    );

    // Unstake some tokens to make it inactive again.
    let msg = snip20_stake::msg::ExecuteMsg::Unstake {
        auth: Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        amount: Uint128::new(50),
    };
    app.execute_contract(
        Addr::unchecked(CREATOR_ADDR),
        &ContractInfo {
            address: staking_contract.addr.clone(),
            code_hash: staking_contract.code_hash.clone(),
        },
        &msg,
        &[],
    )
    .unwrap();
    app.update_block(next_block);

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(CREATOR_ADDR),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Propose(ProposeMsg {
                title: "title".to_string(),
                description: "description".to_string(),
                msgs: vec![],
                proposer: None,
            }),
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert!(matches!(err, ContractError::InactiveDao {}));
}

#[test]
fn test_active_threshold_percent() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.pre_propose_info = PreProposeInfo::AnyoneMayPropose {};
    let core_contract_info = instantiate_with_staking_active_threshold(
        &mut app,
        instantiate,
        None,
        Some(ActiveThreshold::Percentage {
            percent: Decimal::percent(20),
        }),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let voting_module = query_voting_module(
        &app,
        &core_contract_info.address,
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

    let staking_contract: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_module.code_hash.clone(),
            voting_module.addr.clone(),
            &dao_voting_snip20_staked::msg::QueryMsg::StakingContract {},
        )
        .unwrap();

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(CREATOR_ADDR),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Propose(ProposeMsg {
                title: "title".to_string(),
                description: "description".to_string(),
                msgs: vec![],
                proposer: None,
            }),
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert!(matches!(err, ContractError::InactiveDao {}));

    let msg = snip20_reference_impl::msg::ExecuteMsg::Send {
        recipient: staking_contract.addr.clone().to_string(),
        amount: Uint128::new(20_000_000),
        msg: Some(
            to_binary(&snip20_stake::msg::ReceiveMsg::Stake {
                auth: Box::new(Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: CREATOR_ADDR.into(),
                }),
            })
            .unwrap(),
        ),
        recipient_code_hash: Some(staking_contract.code_hash.clone()),
        memo: None,
        decoys: None,
        entropy: None,
        padding: None,
    };
    app.execute_contract(
        Addr::unchecked(CREATOR_ADDR),
        &ContractInfo {
            address: gov_token_info.addr.clone(),
            code_hash: gov_token_info.code_hash.clone(),
        },
        &msg,
        &[],
    )
    .unwrap();
    app.update_block(next_block);

    // Proposal creation now works as tokens have been staked to reach
    // active threshold.
    make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![],
    );

    // Unstake some tokens to make it inactive again.
    let msg = snip20_stake::msg::ExecuteMsg::Unstake {
        amount: Uint128::new(1), // Only one is needed as we're right
        auth: Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        }, // on the edge. :)
    };
    app.execute_contract(
        Addr::unchecked(CREATOR_ADDR),
        &ContractInfo {
            address: staking_contract.addr.clone(),
            code_hash: staking_contract.code_hash.clone(),
        },
        &msg,
        &[],
    )
    .unwrap();
    app.update_block(next_block);

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(CREATOR_ADDR),
            &ContractInfo {
                address: proposal_module.addr,
                code_hash: proposal_module.code_hash,
            },
            &ExecuteMsg::Propose(ProposeMsg {
                title: "title".to_string(),
                description: "description".to_string(),
                msgs: vec![],
                proposer: None,
            }),
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert!(matches!(err, ContractError::InactiveDao {}));
}

#[test]
#[should_panic(
    expected = "min_voting_period and max_voting_period must have the same units (height or time)"
)]
fn test_min_duration_unit_missmatch() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.min_voting_period = Some(Duration::Height(10));
    instantiate_with_staked_balances_governance(&mut app, instantiate, None);
}

#[test]
#[should_panic(expected = "Min voting period must be less than or equal to max voting period")]
fn test_min_duration_larger_than_proposal_duration() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.min_voting_period = Some(Duration::Time(604801));
    instantiate_with_staked_balances_governance(&mut app, instantiate, None);
}

#[test]
fn test_min_voting_period_no_early_pass() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.min_voting_period = Some(Duration::Height(10));
    instantiate.max_voting_period = Duration::Height(100);
    let core_contract_info =
        instantiate_with_staked_balances_governance(&mut app, instantiate, None);
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![],
    );
    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    let proposal_response = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal_response.proposal.status, Status::Open);

    app.update_block(|block| block.height += 10);
    let proposal_response = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash,
        proposal_id,
    );
    assert_eq!(proposal_response.proposal.status, Status::Passed);
}

// Setting the min duration the same as the proposal duration just
// means that proposals cant close early.
#[test]
fn test_min_duration_same_as_proposal_duration() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.min_voting_period = Some(Duration::Height(100));
    instantiate.max_voting_period = Duration::Height(100);
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        instantiate,
        Some(vec![
            InitialBalance {
                address: "ekez".to_string(),
                amount: Uint128::new(10),
            },
            InitialBalance {
                address: "whale".to_string(),
                amount: Uint128::new(90),
            },
        ]),
    );
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
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

    let viewing_key_ekez = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("ekez", &[]),
    );

    let viewing_key_whale = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("whale", &[]),
    );

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        "ekez",
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        "ekez",
        Auth::ViewingKey {
            key: viewing_key_ekez.clone(),
            address: "ekez".into(),
        },
        vec![],
    );

    // Whale votes yes. Normally the proposal would just pass and ekez
    // would be out of luck.
    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        "whale",
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key_whale,
            address: "whale".into(),
        },
        Vote::Yes,
    );
    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        "ekez",
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key_ekez.clone(),
            address: "ekez".into(),
        },
        Vote::No,
    );

    app.update_block(|b| b.height += 100);
    let proposal_response = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash,
        proposal_id,
    );
    assert_eq!(proposal_response.proposal.status, Status::Passed);
}

#[test]
fn test_revoting_playthrough() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.allow_revoting = true;
    let core_contract_info =
        instantiate_with_staked_balances_governance(&mut app, instantiate, None);
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![],
    );

    // Vote and change our minds a couple times.
    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    let proposal_response = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal_response.proposal.status, Status::Open);

    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::No,
    );
    let proposal_response = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal_response.proposal.status, Status::Open);

    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    let proposal_response = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal_response.proposal.status, Status::Open);

    // Can't cast the same vote more than once.
    let err = vote_on_proposal_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
        Vote::Yes,
    );
    assert!(matches!(err, ContractError::AlreadyCast {}));

    // Expire the proposal allowing the votes to be tallied.
    app.update_block(|b| b.time = b.time.plus_seconds(604800));
    let proposal_response = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal_response.proposal.status, Status::Passed);
    execute_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );

    // Can't vote once the proposal is passed.
    let err = vote_on_proposal_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash,
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
        Vote::Yes,
    );
    assert!(matches!(err, ContractError::Expired { .. }));
}

/// Tests that revoting is stored at a per-proposal level. Proposals
/// created while revoting is enabled should not have it disabled if a
/// config change turns if off.
#[test]
fn test_allow_revoting_config_changes() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.allow_revoting = true;
    let core_contract_info =
        instantiate_with_staked_balances_governance(&mut app, instantiate, None);
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    // This proposal should have revoting enable for its entire
    // lifetime.
    let revoting_proposal = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![],
    );

    // Update the config of the proposal module to disable revoting.
    app.execute_contract(
        core_contract_info.address.clone(),
        &ContractInfo {
            address: proposal_module.addr.clone(),
            code_hash: proposal_module.code_hash.clone(),
        },
        &ExecuteMsg::UpdateConfig {
            veto: None,
            threshold: Threshold::ThresholdQuorum {
                quorum: PercentageThreshold::Percent(Decimal::percent(15)),
                threshold: PercentageThreshold::Majority {},
            },
            max_voting_period: Duration::Height(10),
            min_voting_period: None,
            only_members_execute: true,
            // Turn off revoting.
            allow_revoting: false,
            close_proposal_on_execution_failure: false,
        },
        &[],
    )
    .unwrap();

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let no_revoting_proposal = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![],
    );

    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        revoting_proposal,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );
    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        no_revoting_proposal,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::Yes,
    );

    // Proposal without revoting should have passed.
    let proposal_resp = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        no_revoting_proposal,
    );
    assert_eq!(proposal_resp.proposal.status, Status::Passed);

    // Proposal with revoting should not have passed.
    let proposal_resp = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        revoting_proposal,
    );
    assert_eq!(proposal_resp.proposal.status, Status::Open);

    // Can change vote on the revoting proposal.
    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        revoting_proposal,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        Vote::No,
    );
    // Expire the revoting proposal and close it.
    app.update_block(|b| b.time = b.time.plus_seconds(604800));
    close_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash,
        CREATOR_ADDR,
        revoting_proposal,
    );
}

/// Tests a simple three of five multisig configuration.
#[test]
fn test_three_of_five_multisig() {
    let mut app = App::default();
    let mut instantiate = get_default_non_token_dao_proposal_module_instantiate(&mut app);
    instantiate.threshold = Threshold::AbsoluteCount {
        threshold: Uint128::new(3),
    };
    instantiate.pre_propose_info = PreProposeInfo::AnyoneMayPropose {};
    let core_contract_info = instantiate_with_cw4_groups_governance(
        &mut app,
        instantiate,
        Some(vec![
            InitialBalance {
                address: "one".to_string(),
                amount: Uint128::new(1),
            },
            InitialBalance {
                address: "two".to_string(),
                amount: Uint128::new(1),
            },
            InitialBalance {
                address: "three".to_string(),
                amount: Uint128::new(1),
            },
            InitialBalance {
                address: "four".to_string(),
                amount: Uint128::new(1),
            },
            InitialBalance {
                address: "five".to_string(),
                amount: Uint128::new(1),
            },
        ]),
    );

    let core_state: dao_interface::query::DumpStateResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &dao_interface::msg::QueryMsg::DumpState {},
        )
        .unwrap();
    let proposal_module_address = core_state
        .proposal_modules
        .clone()
        .into_iter()
        .next()
        .unwrap()
        .address;

    let proposal_module_codehash = core_state
        .proposal_modules
        .into_iter()
        .next()
        .unwrap()
        .code_hash;

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

    let viewing_key_one = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("one", &[]),
    );

    let viewing_key_two = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("two", &[]),
    );

    let viewing_key_three = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("three", &[]),
    );

    let viewing_key_four = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("four", &[]),
    );

    let proposal_id = make_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![],
    );

    vote_on_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "one",
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key_one.clone(),
            address: "one".into(),
        },
        Vote::Yes,
    );
    vote_on_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "two",
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key_two.clone(),
            address: "two".into(),
        },
        Vote::Yes,
    );

    // Make sure it doesn't pass early.
    let proposal: ProposalResponse = query_proposal(
        &app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        1,
    );
    assert_eq!(proposal.proposal.status, Status::Open);

    vote_on_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "three",
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key_three.clone(),
            address: "three".into(),
        },
        Vote::Yes,
    );

    let proposal: ProposalResponse = query_proposal(
        &app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        1,
    );
    assert_eq!(proposal.proposal.status, Status::Passed);

    execute_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "four",
        Auth::ViewingKey {
            key: viewing_key_four.clone(),
            address: "four".into(),
        },
        proposal_id,
    );

    let proposal: ProposalResponse = query_proposal(
        &app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        1,
    );
    assert_eq!(proposal.proposal.status, Status::Executed);

    // Make another proposal which we'll reject.
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "one",
        Auth::ViewingKey {
            key: viewing_key_one.clone(),
            address: "one".into(),
        },
        vec![],
    );

    vote_on_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "one",
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key_one.clone(),
            address: "one".into(),
        },
        Vote::Yes,
    );
    vote_on_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "two",
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key_two.clone(),
            address: "two".into(),
        },
        Vote::No,
    );
    vote_on_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "three",
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key_three.clone(),
            address: "three".into(),
        },
        Vote::No,
    );
    vote_on_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "four",
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key_four.clone(),
            address: "four".into(),
        },
        Vote::No,
    );

    let proposal = query_proposal(
        &app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Rejected);

    close_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "four",
        proposal_id,
    );
    let proposal = query_proposal(
        &app,
        &proposal_module_address,
        proposal_module_codehash,
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Closed);
}

#[test]
fn test_three_of_five_multisig_revoting() {
    let mut app = App::default();
    let mut instantiate = get_default_non_token_dao_proposal_module_instantiate(&mut app);
    instantiate.threshold = Threshold::AbsoluteCount {
        threshold: Uint128::new(3),
    };
    instantiate.allow_revoting = true;
    instantiate.pre_propose_info = PreProposeInfo::AnyoneMayPropose {};
    let core_contract_info = instantiate_with_cw4_groups_governance(
        &mut app,
        instantiate,
        Some(vec![
            InitialBalance {
                address: "one".to_string(),
                amount: Uint128::new(1),
            },
            InitialBalance {
                address: "two".to_string(),
                amount: Uint128::new(1),
            },
            InitialBalance {
                address: "three".to_string(),
                amount: Uint128::new(1),
            },
            InitialBalance {
                address: "four".to_string(),
                amount: Uint128::new(1),
            },
            InitialBalance {
                address: "five".to_string(),
                amount: Uint128::new(1),
            },
        ]),
    );

    let core_state: dao_interface::query::DumpStateResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &dao_interface::msg::QueryMsg::DumpState {},
        )
        .unwrap();
    let proposal_module_address = core_state
        .proposal_modules
        .clone()
        .into_iter()
        .next()
        .unwrap()
        .address;

    let proposal_module_codehash = core_state
        .proposal_modules
        .into_iter()
        .next()
        .unwrap()
        .code_hash;

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

    let viewing_key_one = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("one", &[]),
    );

    let viewing_key_two = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("two", &[]),
    );

    let viewing_key_three = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("three", &[]),
    );

    let proposal_id = make_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![],
    );

    vote_on_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "one",
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key_one.clone(),
            address: "one".into(),
        },
        Vote::Yes,
    );
    vote_on_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "two",
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key_two.clone(),
            address: "two".into(),
        },
        Vote::Yes,
    );

    // Make sure it doesn't pass early.
    let proposal: ProposalResponse = query_proposal(
        &app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Open);

    vote_on_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "three",
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key_three.clone(),
            address: "three".into(),
        },
        Vote::Yes,
    );

    // Revoting is enabled so the proposal is still open.
    let proposal: ProposalResponse = query_proposal(
        &app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Open);

    // Change our minds.
    vote_on_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "one",
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key_one.clone(),
            address: "one".into(),
        },
        Vote::No,
    );
    vote_on_proposal(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "two",
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key_two.clone(),
            address: "two".into(),
        },
        Vote::No,
    );

    let err = vote_on_proposal_should_fail(
        &mut app,
        &proposal_module_address,
        proposal_module_codehash.clone(),
        "two",
        Auth::ViewingKey {
            key: viewing_key_two.clone(),
            address: "two".into(),
        },
        proposal_id,
        Vote::No,
    );
    assert!(matches!(err, ContractError::AlreadyCast {}));

    // Expire the revoting proposal and close it.
    app.update_block(|b| b.time = b.time.plus_seconds(604800));
    let proposal: ProposalResponse = query_proposal(
        &app,
        &proposal_module_address,
        proposal_module_codehash,
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Rejected);
}

/// Tests that absolute count style thresholds work with token style
/// voting.
#[test]
fn test_absolute_count_threshold_non_multisig() {
    do_votes_staked_balances(
        vec![
            TestSingleChoiceVote {
                voter: "one".to_string(),
                position: Vote::Yes,
                weight: Uint128::new(10),
                should_execute: ShouldExecute::Yes,
            },
            TestSingleChoiceVote {
                voter: "two".to_string(),
                position: Vote::No,
                weight: Uint128::new(200),
                should_execute: ShouldExecute::Yes,
            },
            TestSingleChoiceVote {
                voter: "three".to_string(),
                position: Vote::Yes,
                weight: Uint128::new(1),
                should_execute: ShouldExecute::Yes,
            },
        ],
        Threshold::AbsoluteCount {
            threshold: Uint128::new(11),
        },
        Status::Passed,
        None,
    );
}

/// Tests that we do not overflow when faced with really high token /
/// vote supply.
#[test]
fn test_large_absolute_count_threshold() {
    do_votes_staked_balances(
        vec![
            TestSingleChoiceVote {
                voter: "two".to_string(),
                position: Vote::No,
                weight: Uint128::new(1),
                should_execute: ShouldExecute::Yes,
            },
            // Can vote up to expiration time.
            TestSingleChoiceVote {
                voter: "one".to_string(),
                position: Vote::Yes,
                weight: Uint128::new(u128::MAX - 1),
                should_execute: ShouldExecute::Yes,
            },
        ],
        Threshold::AbsoluteCount {
            threshold: Uint128::new(u128::MAX),
        },
        Status::Rejected,
        None,
    );

    do_votes_staked_balances(
        vec![
            TestSingleChoiceVote {
                voter: "one".to_string(),
                position: Vote::Yes,
                weight: Uint128::new(u128::MAX - 1),
                should_execute: ShouldExecute::Yes,
            },
            TestSingleChoiceVote {
                voter: "two".to_string(),
                position: Vote::No,
                weight: Uint128::new(1),
                should_execute: ShouldExecute::Yes,
            },
        ],
        Threshold::AbsoluteCount {
            threshold: Uint128::new(u128::MAX),
        },
        Status::Rejected,
        None,
    );
}

#[test]
fn test_proposal_count_initialized_to_zero() {
    let mut app = App::default();
    let pre_propose_info = get_pre_propose_info(&mut app, None, false);
    let core_contract_info = instantiate_with_staked_balances_governance(
        &mut app,
        InstantiateMsg {
            veto: None,
            threshold: Threshold::ThresholdQuorum {
                threshold: PercentageThreshold::Majority {},
                quorum: PercentageThreshold::Percent(Decimal::percent(10)),
            },
            max_voting_period: Duration::Height(10),
            min_voting_period: None,
            only_members_execute: true,
            allow_revoting: false,
            pre_propose_info,
            close_proposal_on_execution_failure: true,
            dao_code_hash: "todo!()".into(),
            query_auth: None,
        },
        Some(vec![
            InitialBalance {
                address: "ekez".to_string(),
                amount: Uint128::new(10),
            },
            InitialBalance {
                address: "innactive".to_string(),
                amount: Uint128::new(90),
            },
        ]),
    );

    let core_state: dao_interface::query::DumpStateResponse = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &dao_interface::msg::QueryMsg::DumpState {},
        )
        .unwrap();
    let proposal_modules = core_state.proposal_modules;

    assert_eq!(proposal_modules.len(), 1);
    let proposal_single_addr = proposal_modules.clone().into_iter().next().unwrap().address;
    let proposal_single_codehash = proposal_modules
        .clone()
        .into_iter()
        .next()
        .unwrap()
        .code_hash;

    let proposal_count: u64 = app
        .wrap()
        .query_wasm_smart(
            proposal_single_codehash,
            proposal_single_addr,
            &QueryMsg::ProposalCount {},
        )
        .unwrap();
    assert_eq!(proposal_count, 0);
}

#[test]
pub fn test_migrate_updates_version() {
    let mut deps = mock_dependencies();
    secret_cw2::set_contract_version(&mut deps.storage, "my-contract", "1.0.0").unwrap();
    migrate(deps.as_mut(), mock_env(), MigrateMsg {}).unwrap();
    let version = secret_cw2::get_contract_version(&deps.storage).unwrap();
    assert_eq!(version.version, CONTRACT_VERSION);
    assert_eq!(version.contract, CONTRACT_NAME);
}

#[test]
fn test_proposal_too_large() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.pre_propose_info = PreProposeInfo::AnyoneMayPropose {};
    let core_contract_info =
        instantiate_with_staked_balances_governance(&mut app, instantiate, None);
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash,
    );

    let err = app
        .execute_contract(
            Addr::unchecked(CREATOR_ADDR),
            &ContractInfo {
                address: proposal_module.addr,
                code_hash: proposal_module.code_hash,
            },
            &ExecuteMsg::Propose(ProposeMsg {
                title: "".to_string(),
                description: "a".repeat(MAX_PROPOSAL_SIZE as usize),
                msgs: vec![],
                proposer: None,
            }),
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    assert!(matches!(
        err,
        ContractError::ProposalTooLarge {
            size: _,
            max: MAX_PROPOSAL_SIZE
        }
    ))
}

#[test]
fn test_vote_not_registered() {
    let CommonTest {
        mut app,
        core_contract_info: _,
        proposal_module,
        gov_token_info: _,
        proposal_id,
        query_auth_info,
    } = setup_test(vec![]);
    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr,
            code_hash: query_auth_info.code_hash,
        },
        mock_info("ekez", &[]),
    );

    let err = vote_on_proposal_should_fail(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash,
        "ekez",
        Auth::ViewingKey {
            key: viewing_key,
            address: "ekez".into(),
        },
        proposal_id,
        Vote::Yes,
    );
    assert!(matches!(err, ContractError::NotRegistered {}))
}

#[test]
fn test_proposal_creation_permissions() {
    let CommonTest {
        mut app,
        core_contract_info,
        proposal_module,
        gov_token_info: _,
        proposal_id: _,
        query_auth_info,
    } = setup_test(vec![]);

    // Non pre-propose may not propose.
    let err = app
        .execute_contract(
            Addr::unchecked("notprepropose"),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Propose(ProposeMsg {
                title: "title".to_string(),
                description: "description".to_string(),
                msgs: vec![],
                proposer: None,
            }),
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert!(matches!(err, ContractError::Unauthorized {}));

    let proposal_creation_policy = query_creation_policy(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
    );
    let pre_propose = match proposal_creation_policy {
        ProposalCreationPolicy::Anyone {} => panic!("expected a pre-propose module"),
        ProposalCreationPolicy::Module { addr, code_hash: _ } => addr,
    };

    // Proposer may not be none when a pre-propose module is making
    // the proposal.
    let err = app
        .execute_contract(
            pre_propose,
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Propose(ProposeMsg {
                title: "title".to_string(),
                description: "description".to_string(),
                msgs: vec![],
                proposer: None,
            }),
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert!(matches!(err, ContractError::InvalidProposer {}));

    // Allow anyone to propose.
    app.execute_contract(
        core_contract_info.address.clone(),
        &ContractInfo {
            address: proposal_module.addr.clone(),
            code_hash: proposal_module.code_hash.clone(),
        },
        &ExecuteMsg::UpdatePreProposeInfo {
            info: PreProposeInfo::AnyoneMayPropose {},
        },
        &[],
    )
    .unwrap();

    // Proposer must be None when non pre-propose module is making the
    // proposal.
    let err = app
        .execute_contract(
            Addr::unchecked("ekez"),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Propose(ProposeMsg {
                title: "title".to_string(),
                description: "description".to_string(),
                msgs: vec![],
                proposer: Some("ekez".to_string()),
            }),
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert!(matches!(err, ContractError::InvalidProposer {}));

    let viewing_key_ekez = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info("ekez", &[]),
    );
    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    // Works normally.
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        "ekez",
        Auth::ViewingKey {
            key: viewing_key_ekez,
            address: "ekez".into(),
        },
        vec![],
    );
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.proposer, Addr::unchecked("ekez"));
    vote_on_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        Auth::ViewingKey {
            key: viewing_key,
            address: CREATOR_ADDR.into(),
        },
        Vote::No,
    );
    close_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash,
        CREATOR_ADDR,
        proposal_id,
    );
}

#[test]
fn test_query_info() {
    let CommonTest {
        app,
        core_contract_info: _,
        proposal_module,
        gov_token_info: _,
        proposal_id: _,
        ..
    } = setup_test(vec![]);
    let info: InfoResponse = app
        .wrap()
        .query_wasm_smart(
            proposal_module.code_hash,
            proposal_module.addr,
            &QueryMsg::Info {},
        )
        .unwrap();
    assert_eq!(
        info,
        InfoResponse {
            info: ContractVersion {
                contract: CONTRACT_NAME.to_string(),
                version: CONTRACT_VERSION.to_string()
            }
        }
    )
}

/// DAO should be admin of the pre-propose contract despite the fact
/// that the proposal module instantiates it.
#[test]
fn test_pre_propose_admin_is_dao() {
    let CommonTest {
        app,
        proposal_module,
        gov_token_info: _,
        proposal_id: _,
        ..
    } = setup_test(vec![]);

    let proposal_creation_policy = query_creation_policy(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
    );

    // Check that a new creation policy has been birthed.
    let pre_propose: AnyContractInfo = match proposal_creation_policy {
        ProposalCreationPolicy::Anyone {} => panic!("expected a pre-propose module"),
        ProposalCreationPolicy::Module { addr, code_hash } => AnyContractInfo { addr, code_hash },
    };

    let info: ContractInfoResponse = app
        .wrap()
        .query(&cosmwasm_std::QueryRequest::Wasm(WasmQuery::ContractInfo {
            contract_addr: pre_propose.addr.into_string(),
        }))
        .unwrap();
    assert_eq!(info.creator, proposal_module.addr.into_string());
}

// I can add a rationale to my vote. My rational is queryable when
// listing votes. I can later change my rationale.
#[test]
fn test_rationale() {
    let CommonTest {
        mut app,
        proposal_module,
        proposal_id,
        query_auth_info,
        ..
    } = setup_test(vec![]);

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr,
            code_hash: query_auth_info.code_hash,
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    let rationale = Some("i support dog charities".to_string());

    vote_on_proposal_with_rationale(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
        Vote::Yes,
        rationale.clone(),
    );

    let vote = query_vote(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );
    assert_eq!(vote.vote.unwrap().rationale, rationale);

    let rationale_new =
        Some("i did not realize that dog charity was gambling with customer funds".to_string());

    update_rationale(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        proposal_id,
        rationale_new.clone(),
    );

    let vote = query_vote(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash,
        Auth::ViewingKey {
            key: viewing_key,
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );
    assert_eq!(vote.vote.unwrap().rationale, rationale);
}

// Revoting should override any previous rationale. If no new
// rationalle is provided, the old one will be wiped regardless.
#[test]
fn test_rational_clobbered_on_revote() {
    let mut app = App::default();
    let mut instantiate = get_default_token_dao_proposal_module_instantiate(&mut app);
    instantiate.allow_revoting = true;
    let core_contract_info =
        instantiate_with_staked_balances_governance(&mut app, instantiate, None);
    let gov_token_info = query_dao_token(
        &app,
        &core_contract_info.address,
        core_contract_info.code_hash.clone(),
    );
    let proposal_module = query_single_proposal_module(
        &app,
        &core_contract_info.address,
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

    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    let proposal_id = make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        vec![],
    );

    let rationale = Some("to_string".to_string());

    vote_on_proposal_with_rationale(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
        Vote::Yes,
        rationale.clone(),
    );

    let vote = query_vote(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );
    assert_eq!(vote.vote.unwrap().rationale, rationale);

    let rationale = None;

    // revote and clobber.
    vote_on_proposal_with_rationale(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
        Vote::No,
        None,
    );

    let vote = query_vote(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        proposal_id,
    );
    assert_eq!(vote.vote.unwrap().rationale, rationale);
}

// Casting votes is only allowed within the proposal expiration timeframe
#[test]
pub fn test_not_allow_voting_on_expired_proposal() {
    let CommonTest {
        mut app,
        core_contract_info: _,
        proposal_module,
        gov_token_info: _,
        proposal_id,
        query_auth_info,
    } = setup_test(vec![]);

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info(CREATOR_ADDR, &[]),
    );

    // expire the proposal
    app.update_block(|b| b.time = b.time.plus_seconds(604800));
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Rejected);
    assert_eq!(proposal.proposal.votes.yes, Uint128::zero());

    // attempt to vote past the expiration date
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(CREATOR_ADDR),
            &ContractInfo {
                address: proposal_module.addr.clone(),
                code_hash: proposal_module.code_hash.clone(),
            },
            &ExecuteMsg::Vote {
                proposal_id,
                vote: Vote::Yes,
                rationale: None,
                auth: Auth::ViewingKey {
                    key: viewing_key,
                    address: CREATOR_ADDR.into(),
                },
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    // assert the vote got rejected and did not count
    // towards the votes
    let proposal = query_proposal(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash,
        proposal_id,
    );
    assert_eq!(proposal.proposal.status, Status::Rejected);
    assert_eq!(proposal.proposal.votes.yes, Uint128::zero());
    assert!(matches!(err, ContractError::Expired { id: _proposal_id }));
}

#[test]
fn test_proposal_count_goes_up() {
    let CommonTest {
        mut app,
        proposal_module,
        gov_token_info,
        core_contract_info,
        query_auth_info,
        ..
    } = setup_test(vec![]);

    let next = query_next_proposal_id(
        &app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
    );
    assert_eq!(next, 2);

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.addr.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        mock_info(CREATOR_ADDR, &[]),
    );
    mint_snip20s(
        &mut app,
        &gov_token_info.addr,
        gov_token_info.code_hash.clone(),
        &core_contract_info.address,
        CREATOR_ADDR,
        10_000_000,
    );
    make_proposal(
        &mut app,
        &proposal_module.addr,
        proposal_module.code_hash.clone(),
        CREATOR_ADDR,
        shade_protocol::basic_staking::Auth::ViewingKey {
            key: viewing_key,
            address: CREATOR_ADDR.into(),
        },
        vec![],
    );

    let next = query_next_proposal_id(&app, &proposal_module.addr, proposal_module.code_hash);
    assert_eq!(next, 3);
}
