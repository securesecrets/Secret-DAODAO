use cosmwasm_std::{
    coins, from_binary, to_binary, Addr, Binary, Coin, ContractInfo, Empty, Uint128,
};
use cw_denom::UncheckedDenom;
use dao_interface::state::{Admin, ModuleInstantiateInfo};
use dao_interface::state::{AnyContractInfo, ProposalModule};
use dao_pre_propose_base::{error::PreProposeError, msg::DepositInfoResponse, state::Config};
use dao_proposal_single::query::ProposalResponse;
use dao_voting::{
    deposit::{CheckedDepositInfo, DepositRefundPolicy, DepositToken, UncheckedDepositInfo},
    pre_propose::{PreProposeInfo, ProposalCreationPolicy},
    status::Status,
    threshold::{PercentageThreshold, Threshold},
    voting::Vote,
};
use dao_voting_cw4::msg::GroupContract;
use secret_multi_test::{
    App, BankSudo, Contract, ContractInstantiationInfo, ContractWrapper, Executor,
};
use secret_utils::Duration;
use shade_protocol::basic_staking::Auth;
use shade_protocol::utils::asset::RawContract;
use snip20_base::msg::{InitConfig, InitialBalance};

use crate::state::{Proposal, ProposalStatus};
use crate::{contract::*, msg::*};

const CREATOR_ADDR: &str = "creator";

fn cw_dao_proposal_single_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_proposal_single::contract::execute,
        dao_proposal_single::contract::instantiate,
        dao_proposal_single::contract::query,
    )
    .with_migrate(dao_proposal_single::contract::migrate)
    .with_reply(dao_proposal_single::contract::reply);
    Box::new(contract)
}

pub fn dao_dao_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_dao_core::contract::execute,
        dao_dao_core::contract::instantiate,
        dao_dao_core::contract::query,
    )
    .with_reply(dao_dao_core::contract::reply)
    .with_migrate(dao_dao_core::contract::migrate);
    Box::new(contract)
}

fn cw_pre_propose_base_proposal_single() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(execute, instantiate, query);
    Box::new(contract)
}

fn snip20_base_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip20_base::contract::execute,
        snip20_base::contract::instantiate,
        snip20_base::contract::query,
    );
    Box::new(contract)
}

pub fn cw4_group_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cw4_group::contract::execute,
        cw4_group::contract::instantiate,
        cw4_group::contract::query,
    );
    Box::new(contract)
}

pub fn dao_voting_cw4_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dao_voting_cw4::contract::execute,
        dao_voting_cw4::contract::instantiate,
        dao_voting_cw4::contract::query,
    )
    .with_reply(dao_voting_cw4::contract::reply);
    Box::new(contract)
}

pub fn query_auth_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        query_auth::contract::execute,
        query_auth::contract::instantiate,
        query_auth::contract::query,
    );
    Box::new(contract)
}

pub fn instantiate_with_cw4_groups_governance(
    app: &mut App,
    core_info: ContractInstantiationInfo,
    core_code_id: u64,
    core_code_hash: String,
    proposal_module_instantiate: Binary,
    initial_weights: Option<Vec<InitialBalance>>,
) -> ContractInfo {
    let cw4_info = app.store_code(cw4_group_contract());
    let votemod_info = app.store_code(dao_voting_cw4_contract());
    let query_auth = app.store_code(query_auth_contract());
    let initial_weights = initial_weights.unwrap_or_default();

    // Remove duplicates so that we can test duplicate voting.
    let initial_weights: Vec<cw4::Member> = {
        let mut already_seen = vec![];
        initial_weights
            .into_iter()
            .filter(|InitialBalance { address, .. }| {
                if already_seen.contains(address) {
                    false
                } else {
                    already_seen.push(address.clone());
                    true
                }
            })
            .map(|InitialBalance { address, amount }| cw4::Member {
                addr: address,
                weight: amount.u128() as u64,
            })
            .collect()
    };

    let governance_instantiate = dao_interface::msg::InstantiateMsg {
        dao_uri: None,
        admin: None,
        name: "DAO DAO".to_string(),
        description: "A DAO that builds DAOs".to_string(),
        image_url: None,
        voting_module_instantiate_info: ModuleInstantiateInfo {
            code_id: votemod_info.code_id,
            code_hash: votemod_info.code_hash,
            msg: to_binary(&dao_voting_cw4::msg::InstantiateMsg {
                group_contract: GroupContract::New {
                    cw4_group_code_id: cw4_info.code_id,
                    cw4_group_code_hash: cw4_info.code_hash,
                    initial_members: initial_weights,
                    query_auth: None,
                },
            })
            .unwrap(),
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO voting module".to_string(),
        },
        proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
            code_id: core_code_id,
            code_hash: core_code_hash,
            msg: proposal_module_instantiate,
            admin: Some(Admin::CoreModule {}),
            funds: vec![],
            label: "DAO DAO governance module".to_string(),
        }],
        initial_items: None,
        query_auth_code_id: query_auth.code_id,
        query_auth_code_hash: query_auth.code_hash,
        prng_seed: "seed".into(),
    };

    let info = app
        .instantiate_contract(
            core_info.clone(),
            Addr::unchecked(CREATOR_ADDR),
            &governance_instantiate,
            &[],
            "DAO DAO",
            None,
        )
        .unwrap();

    // Update the block so that weights appear.
    app.update_block(|block| block.height += 1);

    info
}

fn get_default_proposal_module_instantiate(
    app: &mut App,
    deposit_info: Option<UncheckedDepositInfo>,
    open_proposal_submission: bool,
    query_auth: Option<RawContract>,
) -> dao_proposal_single::msg::InstantiateMsg {
    let pre_propose_instantiate_info = app.store_code(cw_pre_propose_base_proposal_single());

    dao_proposal_single::msg::InstantiateMsg {
        threshold: Threshold::AbsolutePercentage {
            percentage: PercentageThreshold::Majority {},
        },
        max_voting_period: secret_utils::Duration::Time(86400),
        min_voting_period: None,
        only_members_execute: false,
        allow_revoting: false,
        pre_propose_info: PreProposeInfo::ModuleMayPropose {
            info: ModuleInstantiateInfo {
                code_id: pre_propose_instantiate_info.code_id,
                code_hash: pre_propose_instantiate_info.code_hash,
                msg: to_binary(&InstantiateMsg {
                    deposit_info,
                    open_proposal_submission,
                    extension: InstantiateExt {
                        approver: "approver".to_string(),
                    },
                })
                .unwrap(),
                admin: Some(Admin::CoreModule {}),
                funds: vec![],
                label: "baby's first pre-propose module".to_string(),
            },
        },
        close_proposal_on_execution_failure: false,
        veto: None,
        query_auth,
    }
}

fn instantiate_snip20_base_default(app: &mut App) -> ContractInfo {
    let snip20_info = app.store_code(snip20_base_contract());
    let snip20_instantiate = snip20_base::msg::InstantiateMsg {
        name: "snip20 token".to_string(),
        symbol: "sniptwenty".to_string(),
        decimals: 6,
        initial_balances: Some(vec![InitialBalance {
            address: "ekez".to_string(),
            amount: Uint128::new(10),
        }]),
        admin: None,
        prng_seed: to_binary("data").unwrap(),
        config: Some(InitConfig {
            public_total_supply: Some(true),
            enable_deposit: Some(true),
            enable_redeem: Some(true),
            enable_mint: Some(true),
            enable_burn: Some(true),
            can_modify_denoms: Some(true),
        }),
        supported_denoms: None,
    };
    app.instantiate_contract(
        snip20_info,
        Addr::unchecked("ekez"),
        &snip20_instantiate,
        &[],
        "snip20-base",
        None,
    )
    .unwrap()
}

struct DefaultTestSetup {
    core_contract_info: ContractInfo,
    proposal_single_contract_info: ContractInfo,
    pre_propose_contract_info: ContractInfo,
}

fn setup_default_test(
    app: &mut App,
    deposit_info: Option<UncheckedDepositInfo>,
    open_proposal_submission: bool,
) -> DefaultTestSetup {
    let dao_proposal_single_instantiate_info = app.store_code(cw_dao_proposal_single_contract());
    let core_contract_instantiate_info = app.store_code(dao_dao_contract());

    let proposal_module_instantiate = get_default_proposal_module_instantiate(
        app,
        deposit_info,
        open_proposal_submission,
        None,
    );

    let core_contract_info = instantiate_with_cw4_groups_governance(
        app,
        core_contract_instantiate_info,
        dao_proposal_single_instantiate_info.code_id,
        dao_proposal_single_instantiate_info.code_hash,
        to_binary(&proposal_module_instantiate).unwrap(),
        Some(vec![
            InitialBalance {
                address: "ekez".to_string(),
                amount: Uint128::new(9),
            },
            InitialBalance {
                address: "keze".to_string(),
                amount: Uint128::new(8),
            },
        ]),
    );
    let proposal_modules: Vec<ProposalModule> = app
        .wrap()
        .query_wasm_smart(
            core_contract_info.code_hash.clone(),
            core_contract_info.address.clone(),
            &dao_interface::msg::QueryMsg::ProposalModules {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(proposal_modules.len(), 1);
    let proposal_single_address = proposal_modules.clone().into_iter().next().unwrap().address;
    let proposal_single_code_hash = proposal_modules.into_iter().next().unwrap().code_hash;
    let proposal_creation_policy = app
        .wrap()
        .query_wasm_smart(
            proposal_single_code_hash.clone(),
            proposal_single_address.clone(),
            &dao_proposal_single::msg::QueryMsg::ProposalCreationPolicy {},
        )
        .unwrap();

    let (pre_propose_addr, pre_proposse_code_hash) = match proposal_creation_policy {
        ProposalCreationPolicy::Module { addr, code_hash } => (addr, code_hash),
        _ => panic!("expected a module for the proposal creation policy"),
    };

    // Make sure things were set up correctly.
    assert_eq!(
        AnyContractInfo {
            addr: proposal_single_address.clone(),
            code_hash: proposal_single_code_hash.clone(),
        },
        get_proposal_module(
            app,
            pre_propose_addr.clone(),
            pre_proposse_code_hash.clone()
        )
    );
    assert_eq!(
        AnyContractInfo {
            addr: core_contract_info.address.clone(),
            code_hash: core_contract_info.code_hash.clone(),
        },
        get_dao(
            app,
            pre_propose_addr.clone(),
            pre_proposse_code_hash.clone()
        )
    );

    DefaultTestSetup {
        core_contract_info,
        proposal_single_contract_info: ContractInfo {
            address: proposal_single_address,
            code_hash: proposal_single_code_hash,
        },
        pre_propose_contract_info: ContractInfo {
            address: pre_propose_addr,
            code_hash: pre_proposse_code_hash,
        },
    }
}

fn make_pre_proposal(
    app: &mut App,
    pre_propose_contract_info: ContractInfo,
    proposer: &str,
    funds: &[Coin],
    auth: Auth,
) -> u64 {
    app.execute_contract(
        Addr::unchecked(proposer),
        &pre_propose_contract_info.clone(),
        &ExecuteMsg::Propose {
            msg: ProposeMessage::Propose {
                title: "title".to_string(),
                description: "description".to_string(),
                msgs: vec![],
            },
            auth,
        },
        funds,
    )
    .unwrap();

    // Query for pending proposal and return latest id.
    let mut pending: Vec<Proposal> = app
        .wrap()
        .query_wasm_smart(
            pre_propose_contract_info.code_hash,
            pre_propose_contract_info.address,
            &QueryMsg::QueryExtension {
                msg: QueryExt::PendingProposals {
                    start_after: None,
                    limit: None,
                },
            },
        )
        .unwrap();

    // Return last item in ascending list, id is first element of tuple
    pending.pop().unwrap().approval_id
}

fn mint_natives(app: &mut App, receiver: &str, coins: Vec<Coin>) {
    // Mint some ekez tokens for ekez so we can pay the deposit.
    app.sudo(secret_multi_test::SudoMsg::Bank(BankSudo::Mint {
        to_address: receiver.to_string(),
        amount: coins,
    }))
    .unwrap();
}

fn increase_allowance(
    app: &mut App,
    sender: &str,
    receiver: &Addr,
    snip20_contract_info: ContractInfo,
    amount: Uint128,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &snip20_contract_info,
        &snip20_base::msg::ExecuteMsg::IncreaseAllowance {
            spender: receiver.to_string(),
            amount,
            expiration: None,
            padding: None,
        },
        &[],
    )
    .unwrap();
}

fn get_balance_snip20<T: Into<String>, C: Into<String>, U: Into<String>, K: Into<String>>(
    app: &App,
    contract_addr: T,
    code_hash: C,
    address: U,
    key: K,
) -> Uint128 {
    let msg = snip20_base::msg::QueryMsg::Balance {
        address: address.into(),
        key: key.into(),
    };
    let result: snip20_base::msg::QueryAnswer = app
        .wrap()
        .query_wasm_smart(code_hash, contract_addr, &msg)
        .unwrap();
    let mut balance = Uint128::zero();
    if let snip20_base::msg::QueryAnswer::Balance { amount } = result {
        balance = amount;
    }
    balance
}

fn get_balance_native(app: &App, who: &str, denom: &str) -> Uint128 {
    let res = app.wrap().query_balance(who, denom).unwrap();
    res.amount
}

fn vote(
    app: &mut App,
    module_contract_info: ContractInfo,
    sender: &str,
    id: u64,
    position: Vote,
    auth: Auth,
) -> Status {
    app.execute_contract(
        Addr::unchecked(sender),
        &module_contract_info.clone(),
        &dao_proposal_single::msg::ExecuteMsg::Vote {
            proposal_id: id,
            vote: position,
            rationale: None,
            auth,
        },
        &[],
    )
    .unwrap();

    let proposal: ProposalResponse = app
        .wrap()
        .query_wasm_smart(
            module_contract_info.code_hash,
            module_contract_info.address,
            &dao_proposal_single::msg::QueryMsg::Proposal { proposal_id: id },
        )
        .unwrap();

    proposal.proposal.status
}

fn get_config(app: &App, module_addr: Addr, module_code_hash: String) -> Config {
    app.wrap()
        .query_wasm_smart(module_code_hash, module_addr, &QueryMsg::Config {})
        .unwrap()
}

fn get_dao(app: &App, module_addr: Addr, module_code_hash: String) -> AnyContractInfo {
    app.wrap()
        .query_wasm_smart(module_code_hash, module_addr, &QueryMsg::Dao {})
        .unwrap()
}

fn get_proposal_module(app: &App, module_addr: Addr, module_code_hash: String) -> AnyContractInfo {
    app.wrap()
        .query_wasm_smart(module_code_hash, module_addr, &QueryMsg::ProposalModule {})
        .unwrap()
}

fn query_query_auth(app: &App, module_addr: Addr, module_code_hash: String) -> AnyContractInfo {
    app.wrap()
        .query_wasm_smart(
            module_code_hash,
            module_addr,
            &dao_interface::msg::QueryMsg::QueryAuthInfo {},
        )
        .unwrap()
}

fn get_deposit_info(
    app: &App,
    module_addr: Addr,
    module_code_hash: String,
    id: u64,
) -> DepositInfoResponse {
    app.wrap()
        .query_wasm_smart(
            module_code_hash,
            module_addr,
            &QueryMsg::DepositInfo { proposal_id: id },
        )
        .unwrap()
}

fn update_config(
    app: &mut App,
    module_contract_info: ContractInfo,
    sender: &str,
    deposit_info: Option<UncheckedDepositInfo>,
    open_proposal_submission: bool,
) -> Config {
    app.execute_contract(
        Addr::unchecked(sender),
        &module_contract_info.clone(),
        &ExecuteMsg::UpdateConfig {
            deposit_info,
            open_proposal_submission,
        },
        &[],
    )
    .unwrap();

    get_config(
        app,
        module_contract_info.address,
        module_contract_info.code_hash,
    )
}

fn update_config_should_fail(
    app: &mut App,
    module_contract_info: ContractInfo,
    sender: &str,
    deposit_info: Option<UncheckedDepositInfo>,
    open_proposal_submission: bool,
) -> PreProposeError {
    app.execute_contract(
        Addr::unchecked(sender),
        &module_contract_info,
        &ExecuteMsg::UpdateConfig {
            deposit_info,
            open_proposal_submission,
        },
        &[],
    )
    .unwrap_err()
    .downcast()
    .unwrap()
}

fn create_viewing_key(app: &mut App, contract_info: ContractInfo, sender: &str) -> String {
    let msg = shade_protocol::contract_interfaces::query_auth::ExecuteMsg::CreateViewingKey {
        entropy: "entropy".to_string(),
        padding: None,
    };
    let res = app
        .execute_contract(Addr::unchecked(sender), &contract_info, &msg, &[])
        .unwrap();
    let mut viewing_key = String::new();
    let data: shade_protocol::contract_interfaces::query_auth::ExecuteAnswer =
        from_binary(&res.data.unwrap()).unwrap();
    if let shade_protocol::contract_interfaces::query_auth::ExecuteAnswer::CreateViewingKey {
        key,
    } = data
    {
        viewing_key = key;
    };
    viewing_key
}

fn create_viewing_key_snip20(app: &mut App, contract_info: ContractInfo, addr: &str) -> String {
    let msg = snip20_base::msg::ExecuteMsg::CreateViewingKey {
        entropy: "entropy".to_string(),
        padding: None,
    };
    let res = app
        .execute_contract(Addr::unchecked(addr), &contract_info, &msg, &[])
        .unwrap();
    let mut viewing_key = String::new();
    let data: snip20_base::msg::ExecuteAnswer = from_binary(&res.data.unwrap()).unwrap();
    if let snip20_base::msg::ExecuteAnswer::CreateViewingKey { key } = data {
        viewing_key = key;
    };
    viewing_key
}

fn _withdraw(
    app: &mut App,
    module_contract_info: ContractInfo,
    sender: &str,
    denom: Option<UncheckedDenom>,
    key: String,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &module_contract_info,
        &ExecuteMsg::Withdraw {
            denom,
            key: Some(key),
        },
        &[],
    )
    .unwrap();
}

fn _withdraw_should_fail(
    app: &mut App,
    module_contract_info: ContractInfo,
    sender: &str,
    denom: Option<UncheckedDenom>,
) -> PreProposeError {
    app.execute_contract(
        Addr::unchecked(sender),
        &module_contract_info,
        &ExecuteMsg::Withdraw { denom, key: None },
        &[],
    )
    .unwrap_err()
    .downcast()
    .unwrap()
}
fn close_proposal(
    app: &mut App,
    module_contract_info: ContractInfo,
    sender: &str,
    proposal_id: u64,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &module_contract_info,
        &dao_proposal_single::msg::ExecuteMsg::Close { proposal_id },
        &[],
    )
    .unwrap();
}

fn close_proposal_wrapper(
    app: &mut App,
    contract_info: ContractInfo,
    sender: &str,
    id: u64,
    _auth: Auth,
) {
    close_proposal(app, contract_info, sender, id);
}

fn execute_proposal(
    app: &mut App,
    module_contract_info: ContractInfo,
    sender: &str,
    proposal_id: u64,
    auth: Auth,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &module_contract_info,
        &dao_proposal_single::msg::ExecuteMsg::Execute { auth, proposal_id },
        &[],
    )
    .unwrap();
}

fn approve_proposal(
    app: &mut App,
    module_contract_info: ContractInfo,
    sender: &str,
    proposal_id: u64,
) -> u64 {
    let res = app
        .execute_contract(
            Addr::unchecked(sender),
            &module_contract_info,
            &ExecuteMsg::Extension {
                msg: ExecuteExt::Approve { id: proposal_id },
            },
            &[],
        )
        .unwrap();

    // Parse attrs from approve_proposal response
    let attrs = res.custom_attrs(res.events.len() - 1);
    // Return ID
    attrs[attrs.len() - 2].value.parse().unwrap()
}

fn reject_proposal(
    app: &mut App,
    module_contract_info: ContractInfo,
    sender: &str,
    proposal_id: u64,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &module_contract_info,
        &ExecuteMsg::Extension {
            msg: ExecuteExt::Reject { id: proposal_id },
        },
        &[],
    )
    .unwrap();
}

enum ApprovalStatus {
    Approved,
    Rejected,
}

enum EndStatus {
    Passed,
    Failed,
}

enum RefundReceiver {
    Proposer,
    Dao,
}

fn test_native_permutation(
    end_status: EndStatus,
    refund_policy: DepositRefundPolicy,
    receiver: RefundReceiver,
    approval_status: ApprovalStatus,
) {
    let mut app = App::default();

    let DefaultTestSetup {
        core_contract_info,
        proposal_single_contract_info,
        pre_propose_contract_info,
    } = setup_default_test(
        &mut app,
        Some(UncheckedDepositInfo {
            denom: DepositToken::Token {
                denom: UncheckedDenom::Native("ujuno".to_string()),
            },
            amount: Uint128::new(10),
            refund_policy,
        }),
        false,
    );

    let query_auth = query_query_auth(
        &app,
        core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );
    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.addr,
            code_hash: query_auth.code_hash,
        },
        "ekez",
    );

    mint_natives(&mut app, "ekez", coins(10, "ujuno"));
    let pre_propose_id = make_pre_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "ekez",
        &coins(10, "ujuno"),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
    );

    // Make sure it went away.
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(balance, Uint128::zero());

    // Approver approves or rejects proposal
    match approval_status {
        ApprovalStatus::Approved => {
            // Approver approves, new proposal id is returned
            let id = approve_proposal(
                &mut app,
                pre_propose_contract_info.clone(),
                "approver",
                pre_propose_id,
            );

            println!("Here :::::::::::::");

            // Voting happens on newly created proposal
            #[allow(clippy::type_complexity)]
            let (position, expected_status, trigger_refund): (
                _,
                _,
                fn(&mut App, ContractInfo, &str, u64, Auth) -> (),
            ) = match end_status {
                EndStatus::Passed => (Vote::Yes, Status::Passed, execute_proposal),
                EndStatus::Failed => (Vote::No, Status::Rejected, close_proposal_wrapper),
            };
            let new_status = vote(
                &mut app,
                proposal_single_contract_info.clone(),
                "ekez",
                id,
                position,
                Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: "ekez".into(),
                },
            );
            assert_eq!(new_status, expected_status);

            // Close or execute the proposal to trigger a refund.
            trigger_refund(
                &mut app,
                proposal_single_contract_info,
                "ekez",
                id,
                Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: "ekez".into(),
                },
            );
        }
        ApprovalStatus::Rejected => {
            // Proposal is rejected by approver
            // No proposal is created so there is no voting
            reject_proposal(
                &mut app,
                pre_propose_contract_info,
                "approver",
                pre_propose_id,
            );
        }
    };

    let (dao_expected, proposer_expected) = match receiver {
        RefundReceiver::Proposer => (0, 10),
        RefundReceiver::Dao => (10, 0),
    };

    let proposer_balance = get_balance_native(&app, "ekez", "ujuno");
    let dao_balance = get_balance_native(&app, core_contract_info.address.as_str(), "ujuno");
    assert_eq!(proposer_expected, proposer_balance.u128());
    assert_eq!(dao_expected, dao_balance.u128())
}

fn test_snip20_permutation(
    end_status: EndStatus,
    refund_policy: DepositRefundPolicy,
    receiver: RefundReceiver,
    approval_status: ApprovalStatus,
) {
    let mut app = App::default();

    let snip20_contract_info = instantiate_snip20_base_default(&mut app);

    let DefaultTestSetup {
        core_contract_info,
        proposal_single_contract_info,
        pre_propose_contract_info,
    } = setup_default_test(
        &mut app,
        Some(UncheckedDepositInfo {
            denom: DepositToken::Token {
                denom: UncheckedDenom::Snip20(
                    snip20_contract_info.address.to_string(),
                    snip20_contract_info.code_hash.clone(),
                ),
            },
            amount: Uint128::new(10),
            refund_policy,
        }),
        false,
    );

    let query_auth = query_query_auth(
        &app,
        core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );
    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.addr,
            code_hash: query_auth.code_hash,
        },
        "ekez",
    );

    let viewing_key_snip20 = create_viewing_key_snip20(
        &mut app,
        ContractInfo {
            address: snip20_contract_info.address.clone(),
            code_hash: snip20_contract_info.code_hash.clone(),
        },
        "ekez",
    );

    let viewing_key_snip20_core = create_viewing_key_snip20(
        &mut app,
        ContractInfo {
            address: snip20_contract_info.address.clone(),
            code_hash: snip20_contract_info.code_hash.clone(),
        },
        core_contract_info.address.as_str(),
    );

    increase_allowance(
        &mut app,
        "ekez",
        &pre_propose_contract_info.address.clone(),
        snip20_contract_info.clone(),
        Uint128::new(10),
    );
    let pre_propose_id = make_pre_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "ekez",
        &[],
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
    );

    // Make sure it went await.
    let balance = get_balance_snip20(
        &app,
        snip20_contract_info.address.clone(),
        snip20_contract_info.code_hash.clone(),
        "ekez",
        viewing_key_snip20.clone(),
    );
    assert_eq!(balance, Uint128::zero());

    // Approver approves or rejects proposal
    match approval_status {
        ApprovalStatus::Approved => {
            // Approver approves, new proposal id is returned
            let id = approve_proposal(
                &mut app,
                pre_propose_contract_info.clone(),
                "approver",
                pre_propose_id,
            );

            // Voting happens on newly created proposal
            #[allow(clippy::type_complexity)]
            let (position, expected_status, trigger_refund): (
                _,
                _,
                fn(&mut App, ContractInfo, &str, u64, Auth) -> (),
            ) = match end_status {
                EndStatus::Passed => (Vote::Yes, Status::Passed, execute_proposal),
                EndStatus::Failed => (Vote::No, Status::Rejected, close_proposal_wrapper),
            };
            let new_status = vote(
                &mut app,
                proposal_single_contract_info.clone(),
                "ekez",
                id,
                position,
                Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: "ekez".into(),
                },
            );
            assert_eq!(new_status, expected_status);

            // Close or execute the proposal to trigger a refund.
            trigger_refund(
                &mut app,
                proposal_single_contract_info,
                "ekez",
                id,
                Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: "ekez".into(),
                },
            );
        }
        ApprovalStatus::Rejected => {
            // Proposal is rejected by approver
            // No proposal is created so there is no voting
            reject_proposal(
                &mut app,
                pre_propose_contract_info.clone(),
                "approver",
                pre_propose_id,
            );
        }
    };

    let (dao_expected, proposer_expected) = match receiver {
        RefundReceiver::Proposer => (0, 10),
        RefundReceiver::Dao => (10, 0),
    };

    let proposer_balance = get_balance_snip20(
        &app,
        &snip20_contract_info.address.clone(),
        snip20_contract_info.code_hash.clone(),
        "ekez",
        viewing_key_snip20,
    );
    let dao_balance = get_balance_snip20(
        &app,
        &snip20_contract_info.address,
        snip20_contract_info.code_hash,
        core_contract_info.address.as_str(),
        viewing_key_snip20_core,
    );
    assert_eq!(proposer_expected, proposer_balance.u128());
    assert_eq!(dao_expected, dao_balance.u128())
}

#[test]
fn test_native_failed_always_refund() {
    test_native_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::Always,
        RefundReceiver::Proposer,
        ApprovalStatus::Approved,
    )
}

#[test]
fn test_native_rejected_always_refund() {
    test_native_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::Always,
        RefundReceiver::Proposer,
        ApprovalStatus::Rejected,
    )
}

#[test]
fn test_snip20_failed_always_refund() {
    test_snip20_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::Always,
        RefundReceiver::Proposer,
        ApprovalStatus::Approved,
    )
}

#[test]
fn test_snip20_rejected_always_refund() {
    test_snip20_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::Always,
        RefundReceiver::Proposer,
        ApprovalStatus::Rejected,
    )
}

#[test]
fn test_native_passed_always_refund() {
    test_native_permutation(
        EndStatus::Passed,
        DepositRefundPolicy::Always,
        RefundReceiver::Proposer,
        ApprovalStatus::Approved,
    )
}

#[test]
fn test_snip20_passed_always_refund() {
    test_snip20_permutation(
        EndStatus::Passed,
        DepositRefundPolicy::Always,
        RefundReceiver::Proposer,
        ApprovalStatus::Approved,
    )
}

#[test]
fn test_native_passed_never_refund() {
    test_native_permutation(
        EndStatus::Passed,
        DepositRefundPolicy::Never,
        RefundReceiver::Dao,
        ApprovalStatus::Approved,
    )
}

#[test]
fn test_snip20_passed_never_refund() {
    test_snip20_permutation(
        EndStatus::Passed,
        DepositRefundPolicy::Never,
        RefundReceiver::Dao,
        ApprovalStatus::Approved,
    )
}

#[test]
fn test_native_failed_never_refund() {
    test_native_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::Never,
        RefundReceiver::Dao,
        ApprovalStatus::Approved,
    )
}

#[test]
fn test_native_rejected_never_refund() {
    test_native_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::Never,
        RefundReceiver::Dao,
        ApprovalStatus::Rejected,
    )
}

#[test]
fn test_snip20_failed_never_refund() {
    test_snip20_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::Never,
        RefundReceiver::Dao,
        ApprovalStatus::Approved,
    )
}

#[test]
fn test_snip20_rejected_never_refund() {
    test_snip20_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::Never,
        RefundReceiver::Dao,
        ApprovalStatus::Rejected,
    )
}

#[test]
fn test_native_passed_passed_refund() {
    test_native_permutation(
        EndStatus::Passed,
        DepositRefundPolicy::OnlyPassed,
        RefundReceiver::Proposer,
        ApprovalStatus::Approved,
    )
}
#[test]
fn test_snip20_passed_passed_refund() {
    test_snip20_permutation(
        EndStatus::Passed,
        DepositRefundPolicy::OnlyPassed,
        RefundReceiver::Proposer,
        ApprovalStatus::Approved,
    )
}

#[test]
fn test_native_failed_passed_refund() {
    test_native_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::OnlyPassed,
        RefundReceiver::Dao,
        ApprovalStatus::Approved,
    )
}

#[test]
fn test_native_rejected_passed_refund() {
    test_native_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::OnlyPassed,
        RefundReceiver::Dao,
        ApprovalStatus::Rejected,
    )
}

#[test]
fn test_snip20_failed_passed_refund() {
    test_snip20_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::OnlyPassed,
        RefundReceiver::Dao,
        ApprovalStatus::Approved,
    )
}

#[test]
fn test_snip20_rejected_passed_refund() {
    test_snip20_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::OnlyPassed,
        RefundReceiver::Dao,
        ApprovalStatus::Rejected,
    )
}

// See: <https://github.com/DA0-DA0/dao-contracts/pull/465#discussion_r960092321>
#[test]
fn test_multiple_open_proposals() {
    let mut app = App::default();

    let DefaultTestSetup {
        core_contract_info,
        proposal_single_contract_info,
        pre_propose_contract_info,
    } = setup_default_test(
        &mut app,
        Some(UncheckedDepositInfo {
            denom: DepositToken::Token {
                denom: UncheckedDenom::Native("ujuno".to_string()),
            },
            amount: Uint128::new(10),
            refund_policy: DepositRefundPolicy::Always,
        }),
        false,
    );

    let query_auth = query_query_auth(
        &app,
        core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );
    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.addr,
            code_hash: query_auth.code_hash,
        },
        "ekez",
    );

    mint_natives(&mut app, "ekez", coins(20, "ujuno"));
    let first_pre_propose_id = make_pre_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "ekez",
        &coins(10, "ujuno"),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
    );
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(10, balance.u128());

    // Approver approves prop, balance remains the same
    let first_id = approve_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "approver",
        first_pre_propose_id,
    );
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(10, balance.u128());

    let second_pre_propose_id = make_pre_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "ekez",
        &coins(10, "ujuno"),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
    );
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(0, balance.u128());

    // Approver approves prop, balance remains the same
    let second_id = approve_proposal(
        &mut app,
        pre_propose_contract_info,
        "approver",
        second_pre_propose_id,
    );
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(0, balance.u128());

    // Finish up the first proposal.
    let new_status = vote(
        &mut app,
        proposal_single_contract_info.clone(),
        "ekez",
        first_id,
        Vote::Yes,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
    );
    assert_eq!(Status::Passed, new_status);

    // Still zero.
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(0, balance.u128());

    execute_proposal(
        &mut app,
        proposal_single_contract_info.clone(),
        "ekez",
        first_id,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
    );

    // First proposal refunded.
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(10, balance.u128());

    // Finish up the second proposal.
    let new_status = vote(
        &mut app,
        proposal_single_contract_info.clone(),
        "ekez",
        second_id,
        Vote::No,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
    );
    assert_eq!(Status::Rejected, new_status);

    // Still zero.
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(10, balance.u128());

    close_proposal(&mut app, proposal_single_contract_info, "ekez", second_id);

    // All deposits have been refunded.
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(20, balance.u128());
}

#[test]
fn test_pending_proposal_queries() {
    let mut app = App::default();

    let DefaultTestSetup {
        core_contract_info,
        proposal_single_contract_info: _,
        pre_propose_contract_info,
    } = setup_default_test(
        &mut app,
        Some(UncheckedDepositInfo {
            denom: DepositToken::Token {
                denom: UncheckedDenom::Native("ujuno".to_string()),
            },
            amount: Uint128::new(10),
            refund_policy: DepositRefundPolicy::Always,
        }),
        false,
    );

    let query_auth = query_query_auth(
        &app,
        core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );
    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.addr,
            code_hash: query_auth.code_hash,
        },
        "ekez",
    );

    mint_natives(&mut app, "ekez", coins(20, "ujuno"));
    make_pre_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "ekez",
        &coins(10, "ujuno"),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
    );
    make_pre_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "ekez",
        &coins(10, "ujuno"),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
    );

    // Query for individual proposal
    let prop1: Proposal = app
        .wrap()
        .query_wasm_smart(
            pre_propose_contract_info.code_hash.clone(),
            pre_propose_contract_info.address.clone(),
            &QueryMsg::QueryExtension {
                msg: QueryExt::PendingProposal { id: 1 },
            },
        )
        .unwrap();
    assert_eq!(prop1.approval_id, 1);
    assert_eq!(prop1.status, ProposalStatus::Pending {});

    let prop1: Proposal = app
        .wrap()
        .query_wasm_smart(
            pre_propose_contract_info.code_hash.clone(),
            pre_propose_contract_info.address.clone(),
            &QueryMsg::QueryExtension {
                msg: QueryExt::Proposal { id: 1 },
            },
        )
        .unwrap();
    assert_eq!(prop1.approval_id, 1);
    assert_eq!(prop1.status, ProposalStatus::Pending {});

    // Query for the pre-propose proposals
    let pre_propose_props: Vec<Proposal> = app
        .wrap()
        .query_wasm_smart(
            pre_propose_contract_info.code_hash.clone(),
            pre_propose_contract_info.address.clone(),
            &QueryMsg::QueryExtension {
                msg: QueryExt::PendingProposals {
                    start_after: None,
                    limit: None,
                },
            },
        )
        .unwrap();
    assert_eq!(pre_propose_props.len(), 2);
    assert_eq!(pre_propose_props[0].approval_id, 1);

    // Query props in reverse
    let reverse_pre_propose_props: Vec<Proposal> = app
        .wrap()
        .query_wasm_smart(
            pre_propose_contract_info.code_hash,
            pre_propose_contract_info.address,
            &QueryMsg::QueryExtension {
                msg: QueryExt::ReversePendingProposals {
                    start_before: None,
                    limit: None,
                },
            },
        )
        .unwrap();

    assert_eq!(reverse_pre_propose_props.len(), 2);
    assert_eq!(reverse_pre_propose_props[0].approval_id, 2);
}

#[test]
fn test_completed_proposal_queries() {
    let mut app = App::default();

    let DefaultTestSetup {
        core_contract_info,
        proposal_single_contract_info: _,
        pre_propose_contract_info,
    } = setup_default_test(
        &mut app,
        Some(UncheckedDepositInfo {
            denom: DepositToken::Token {
                denom: UncheckedDenom::Native("ujuno".to_string()),
            },
            amount: Uint128::new(10),
            refund_policy: DepositRefundPolicy::Always,
        }),
        false,
    );

    let query_auth = query_query_auth(
        &app,
        core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );
    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.addr,
            code_hash: query_auth.code_hash,
        },
        "ekez",
    );

    mint_natives(&mut app, "ekez", coins(20, "ujuno"));
    let approve_id = make_pre_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "ekez",
        &coins(10, "ujuno"),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
    );
    let reject_id = make_pre_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "ekez",
        &coins(10, "ujuno"),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
    );

    let is_pending: bool = app
        .wrap()
        .query_wasm_smart(
            pre_propose_contract_info.code_hash.clone(),
            pre_propose_contract_info.address.clone(),
            &QueryMsg::QueryExtension {
                msg: QueryExt::IsPending { id: approve_id },
            },
        )
        .unwrap();
    assert!(is_pending);

    let created_approved_id = approve_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "approver",
        approve_id,
    );
    reject_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "approver",
        reject_id,
    );

    let is_pending: bool = app
        .wrap()
        .query_wasm_smart(
            pre_propose_contract_info.code_hash.clone(),
            pre_propose_contract_info.address.clone(),
            &QueryMsg::QueryExtension {
                msg: QueryExt::IsPending { id: approve_id },
            },
        )
        .unwrap();
    assert!(!is_pending);

    // Query for individual proposals
    let prop1: Proposal = app
        .wrap()
        .query_wasm_smart(
            pre_propose_contract_info.code_hash.clone(),
            pre_propose_contract_info.address.clone(),
            &QueryMsg::QueryExtension {
                msg: QueryExt::CompletedProposal { id: approve_id },
            },
        )
        .unwrap();
    assert_eq!(
        prop1.status,
        ProposalStatus::Approved {
            created_proposal_id: created_approved_id
        }
    );
    let prop1: Proposal = app
        .wrap()
        .query_wasm_smart(
            pre_propose_contract_info.code_hash.clone(),
            pre_propose_contract_info.address.clone(),
            &QueryMsg::QueryExtension {
                msg: QueryExt::Proposal { id: approve_id },
            },
        )
        .unwrap();
    assert_eq!(
        prop1.status,
        ProposalStatus::Approved {
            created_proposal_id: created_approved_id
        }
    );

    let prop1_id: Option<u64> = app
        .wrap()
        .query_wasm_smart(
            pre_propose_contract_info.code_hash.clone(),
            pre_propose_contract_info.address.clone(),
            &QueryMsg::QueryExtension {
                msg: QueryExt::CompletedProposalIdForCreatedProposalId {
                    id: created_approved_id,
                },
            },
        )
        .unwrap();
    assert_eq!(prop1_id, Some(approve_id));

    let prop2: Proposal = app
        .wrap()
        .query_wasm_smart(
            pre_propose_contract_info.code_hash.clone(),
            pre_propose_contract_info.address.clone(),
            &QueryMsg::QueryExtension {
                msg: QueryExt::CompletedProposal { id: reject_id },
            },
        )
        .unwrap();
    assert_eq!(prop2.status, ProposalStatus::Rejected {});

    // Query for the pre-propose proposals
    let pre_propose_props: Vec<Proposal> = app
        .wrap()
        .query_wasm_smart(
            pre_propose_contract_info.code_hash.clone(),
            pre_propose_contract_info.address.clone(),
            &QueryMsg::QueryExtension {
                msg: QueryExt::CompletedProposals {
                    start_after: None,
                    limit: None,
                },
            },
        )
        .unwrap();
    assert_eq!(pre_propose_props.len(), 2);
    assert_eq!(pre_propose_props[0].approval_id, approve_id);
    assert_eq!(pre_propose_props[1].approval_id, reject_id);

    // Query props in reverse
    let reverse_pre_propose_props: Vec<Proposal> = app
        .wrap()
        .query_wasm_smart(
            pre_propose_contract_info.code_hash,
            pre_propose_contract_info.address,
            &QueryMsg::QueryExtension {
                msg: QueryExt::ReverseCompletedProposals {
                    start_before: None,
                    limit: None,
                },
            },
        )
        .unwrap();

    assert_eq!(reverse_pre_propose_props.len(), 2);
    assert_eq!(reverse_pre_propose_props[0].approval_id, reject_id);
    assert_eq!(reverse_pre_propose_props[1].approval_id, approve_id);
}

#[test]
fn test_permissions() {
    let mut app = App::default();

    let DefaultTestSetup {
        core_contract_info,
        proposal_single_contract_info: _,
        pre_propose_contract_info,
    } = setup_default_test(
        &mut app,
        Some(UncheckedDepositInfo {
            denom: DepositToken::Token {
                denom: UncheckedDenom::Native("ujuno".to_string()),
            },
            amount: Uint128::new(10),
            refund_policy: DepositRefundPolicy::Always,
        }),
        false, // no open proposal submission.
    );

    let query_auth = query_query_auth(
        &app,
        core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );
    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.addr,
            code_hash: query_auth.code_hash,
        },
        "nonmember",
    );

    let err: PreProposeError = app
        .execute_contract(
            core_contract_info.address,
            &pre_propose_contract_info.clone(),
            &ExecuteMsg::ProposalCompletedHook {
                proposal_id: 1,
                new_status: Status::Closed,
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, PreProposeError::NotModule {});

    // Non-members may not propose when open_propose_submission is
    // disabled.
    let err: PreProposeError = app
        .execute_contract(
            Addr::unchecked("nonmember"),
            &pre_propose_contract_info,
            &ExecuteMsg::Propose {
                msg: ProposeMessage::Propose {
                    title: "I would like to join the DAO".to_string(),
                    description: "though, I am currently not a member.".to_string(),
                    msgs: vec![],
                },
                auth: Auth::ViewingKey {
                    key: viewing_key,
                    address: "nonmember".into(),
                },
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, PreProposeError::NotMember {});
}

#[test]
fn test_approval_and_rejection_permissions() {
    let mut app = App::default();
    let DefaultTestSetup {
        core_contract_info,
        proposal_single_contract_info: _,
        pre_propose_contract_info,
    } = setup_default_test(
        &mut app,
        Some(UncheckedDepositInfo {
            denom: DepositToken::Token {
                denom: UncheckedDenom::Native("ujuno".to_string()),
            },
            amount: Uint128::new(10),
            refund_policy: DepositRefundPolicy::Always,
        }),
        true, // yes, open proposal submission.
    );

    let query_auth = query_query_auth(
        &app,
        core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );
    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.addr,
            code_hash: query_auth.code_hash,
        },
        "nonmember",
    );

    // Non-member proposes.
    mint_natives(&mut app, "nonmember", coins(10, "ujuno"));
    let pre_propose_id = make_pre_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "nonmember",
        &coins(10, "ujuno"),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "nonmember".into(),
        },
    );

    // Only approver can propose
    let err: PreProposeError = app
        .execute_contract(
            Addr::unchecked("nonmember"),
            &pre_propose_contract_info.clone(),
            &ExecuteMsg::Extension {
                msg: ExecuteExt::Approve { id: pre_propose_id },
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, PreProposeError::Unauthorized {});

    // Only approver can propose
    let err: PreProposeError = app
        .execute_contract(
            Addr::unchecked("nonmember"),
            &pre_propose_contract_info,
            &ExecuteMsg::Extension {
                msg: ExecuteExt::Reject { id: pre_propose_id },
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, PreProposeError::Unauthorized {});
}

#[test]
fn test_propose_open_proposal_submission() {
    let mut app = App::default();
    let DefaultTestSetup {
        core_contract_info,
        proposal_single_contract_info,
        pre_propose_contract_info,
    } = setup_default_test(
        &mut app,
        Some(UncheckedDepositInfo {
            denom: DepositToken::Token {
                denom: UncheckedDenom::Native("ujuno".to_string()),
            },
            amount: Uint128::new(10),
            refund_policy: DepositRefundPolicy::Always,
        }),
        true, // yes, open proposal submission.
    );

    let query_auth = query_query_auth(
        &app,
        core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );
    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.addr.clone(),
            code_hash: query_auth.code_hash.clone(),
        },
        "nonmember",
    );

    let viewing_key_ekez = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.addr,
            code_hash: query_auth.code_hash,
        },
        "ekez",
    );

    // Non-member proposes.
    mint_natives(&mut app, "nonmember", coins(10, "ujuno"));
    let pre_propose_id = make_pre_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "nonmember",
        &coins(10, "ujuno"),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "nonmember".into(),
        },
    );

    // Approver approves
    let id = approve_proposal(
        &mut app,
        pre_propose_contract_info,
        "approver",
        pre_propose_id,
    );

    // Member votes.
    let new_status = vote(
        &mut app,
        proposal_single_contract_info,
        "ekez",
        id,
        Vote::Yes,
        Auth::ViewingKey {
            key: viewing_key_ekez,
            address: "ekez".into(),
        },
    );
    assert_eq!(Status::Passed, new_status)
}

#[test]
fn test_no_deposit_required_open_submission() {
    let mut app = App::default();
    let DefaultTestSetup {
        core_contract_info,
        proposal_single_contract_info,
        pre_propose_contract_info,
    } = setup_default_test(
        &mut app, None, true, // yes, open proposal submission.
    );

    let query_auth = query_query_auth(
        &app,
        core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );
    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.addr.clone(),
            code_hash: query_auth.code_hash.clone(),
        },
        "nonmember",
    );

    let viewing_key_ekez = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.addr,
            code_hash: query_auth.code_hash,
        },
        "ekez",
    );

    // Non-member proposes.
    let pre_propose_id = make_pre_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "nonmember",
        &[],
        Auth::ViewingKey {
            key: viewing_key,
            address: "nonmember".into(),
        },
    );

    // Approver approves
    let id = approve_proposal(
        &mut app,
        pre_propose_contract_info,
        "approver",
        pre_propose_id,
    );

    // Member votes.
    let new_status = vote(
        &mut app,
        proposal_single_contract_info,
        "ekez",
        id,
        Vote::Yes,
        Auth::ViewingKey {
            key: viewing_key_ekez,
            address: "ekez".into(),
        },
    );
    assert_eq!(Status::Passed, new_status)
}

#[test]
fn test_no_deposit_required_members_submission() {
    let mut app = App::default();
    let DefaultTestSetup {
        core_contract_info,
        proposal_single_contract_info,
        pre_propose_contract_info,
    } = setup_default_test(
        &mut app, None, false, // no open proposal submission.
    );

    let query_auth = query_query_auth(
        &app,
        core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );
    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.addr.clone(),
            code_hash: query_auth.code_hash.clone(),
        },
        "nonmember",
    );

    let viewing_key_ekez = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.addr,
            code_hash: query_auth.code_hash,
        },
        "ekez",
    );

    // Non-member proposes and this fails.
    let err: PreProposeError = app
        .execute_contract(
            Addr::unchecked("nonmember"),
            &pre_propose_contract_info.clone(),
            &ExecuteMsg::Propose {
                msg: ProposeMessage::Propose {
                    title: "I would like to join the DAO".to_string(),
                    description: "though, I am currently not a member.".to_string(),
                    msgs: vec![],
                },
                auth: Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: "nonmember".into(),
                },
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, PreProposeError::NotMember {});

    let pre_propose_id = make_pre_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "ekez",
        &[],
        Auth::ViewingKey {
            key: viewing_key_ekez.clone(),
            address: "ekez".into(),
        },
    );

    // Approver approves
    let id = approve_proposal(
        &mut app,
        pre_propose_contract_info,
        "approver",
        pre_propose_id,
    );

    let new_status = vote(
        &mut app,
        proposal_single_contract_info,
        "ekez",
        id,
        Vote::Yes,
        Auth::ViewingKey {
            key: viewing_key_ekez,
            address: "ekez".into(),
        },
    );
    assert_eq!(Status::Passed, new_status)
}

#[test]
#[should_panic(expected = "invalid zero deposit. set the deposit to `None` to have no deposit")]
fn test_instantiate_with_zero_native_deposit() {
    let mut app = App::default();

    let dao_proposal_single_instantiate_info = app.store_code(cw_dao_proposal_single_contract());
    let core_instantiate_info = app.store_code(dao_dao_contract());

    let proposal_module_instantiate = {
        let pre_propose_instantiate_info = app.store_code(cw_pre_propose_base_proposal_single());

        dao_proposal_single::msg::InstantiateMsg {
            threshold: Threshold::AbsolutePercentage {
                percentage: PercentageThreshold::Majority {},
            },
            max_voting_period: Duration::Time(86400),
            min_voting_period: None,
            only_members_execute: false,
            allow_revoting: false,
            pre_propose_info: PreProposeInfo::ModuleMayPropose {
                info: ModuleInstantiateInfo {
                    code_id: pre_propose_instantiate_info.code_id,
                    msg: to_binary(&InstantiateMsg {
                        deposit_info: Some(UncheckedDepositInfo {
                            denom: DepositToken::Token {
                                denom: UncheckedDenom::Native("ujuno".to_string()),
                            },
                            amount: Uint128::zero(),
                            refund_policy: DepositRefundPolicy::OnlyPassed,
                        }),
                        open_proposal_submission: false,
                        extension: InstantiateExt {
                            approver: "approver".to_string(),
                        },
                    })
                    .unwrap(),
                    admin: Some(Admin::CoreModule {}),
                    funds: vec![],
                    label: "baby's first pre-propose module".to_string(),
                    code_hash: pre_propose_instantiate_info.code_hash.clone(),
                },
            },
            close_proposal_on_execution_failure: false,
            veto: None,
            query_auth: None,
        }
    };

    // Should panic.
    instantiate_with_cw4_groups_governance(
        &mut app,
        core_instantiate_info,
        dao_proposal_single_instantiate_info.code_id,
        dao_proposal_single_instantiate_info.code_hash,
        to_binary(&proposal_module_instantiate).unwrap(),
        Some(vec![
            InitialBalance {
                address: "ekez".to_string(),
                amount: Uint128::new(9),
            },
            InitialBalance {
                address: "keze".to_string(),
                amount: Uint128::new(8),
            },
        ]),
    );
}

#[test]
#[should_panic(expected = "invalid zero deposit. set the deposit to `None` to have no deposit")]
fn test_instantiate_with_zero_snip20_deposit() {
    let mut app = App::default();

    let snip20_contract_info = instantiate_snip20_base_default(&mut app);

    let dao_proposal_single_instantiate_info = app.store_code(cw_dao_proposal_single_contract());
    let core_contract_info = app.store_code(dao_dao_contract());

    let proposal_module_instantiate = {
        let pre_propose_instantiate_info = app.store_code(cw_pre_propose_base_proposal_single());

        dao_proposal_single::msg::InstantiateMsg {
            threshold: Threshold::AbsolutePercentage {
                percentage: PercentageThreshold::Majority {},
            },
            max_voting_period: Duration::Time(86400),
            min_voting_period: None,
            only_members_execute: false,
            allow_revoting: false,
            pre_propose_info: PreProposeInfo::ModuleMayPropose {
                info: ModuleInstantiateInfo {
                    code_id: pre_propose_instantiate_info.code_id,
                    code_hash: pre_propose_instantiate_info.code_hash,
                    msg: to_binary(&InstantiateMsg {
                        deposit_info: Some(UncheckedDepositInfo {
                            denom: DepositToken::Token {
                                denom: UncheckedDenom::Snip20(
                                    snip20_contract_info.address.into_string(),
                                    snip20_contract_info.code_hash,
                                ),
                            },
                            amount: Uint128::zero(),
                            refund_policy: DepositRefundPolicy::OnlyPassed,
                        }),
                        open_proposal_submission: false,
                        extension: InstantiateExt {
                            approver: "approver".to_string(),
                        },
                    })
                    .unwrap(),
                    admin: Some(Admin::CoreModule {}),
                    funds: vec![],
                    label: "baby's first pre-propose module".to_string(),
                },
            },
            close_proposal_on_execution_failure: false,
            veto: None,
            query_auth: None,
        }
    };

    // Should panic.
    instantiate_with_cw4_groups_governance(
        &mut app,
        core_contract_info,
        dao_proposal_single_instantiate_info.code_id,
        dao_proposal_single_instantiate_info.code_hash,
        to_binary(&proposal_module_instantiate).unwrap(),
        Some(vec![
            InitialBalance {
                address: "ekez".to_string(),
                amount: Uint128::new(9),
            },
            InitialBalance {
                address: "keze".to_string(),
                amount: Uint128::new(8),
            },
        ]),
    );
}

#[test]
fn test_update_config() {
    let mut app = App::default();
    let DefaultTestSetup {
        core_contract_info,
        proposal_single_contract_info,
        pre_propose_contract_info,
    } = setup_default_test(&mut app, None, false);

    let config = get_config(
        &app,
        pre_propose_contract_info.address.clone(),
        pre_propose_contract_info.code_hash.clone(),
    );
    assert_eq!(
        config,
        Config {
            deposit_info: None,
            open_proposal_submission: false
        }
    );

    let query_auth = query_query_auth(
        &app,
        core_contract_info.address.clone(),
        core_contract_info.code_hash.clone(),
    );

    let viewing_key_ekez = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.addr,
            code_hash: query_auth.code_hash,
        },
        "ekez",
    );

    let pre_propose_id = make_pre_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "ekez",
        &[],
        Auth::ViewingKey {
            key: viewing_key_ekez.clone(),
            address: "ekez".into(),
        },
    );

    // Approver approves
    let id = approve_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "approver",
        pre_propose_id,
    );

    update_config(
        &mut app,
        pre_propose_contract_info.clone(),
        core_contract_info.address.as_str(),
        Some(UncheckedDepositInfo {
            denom: DepositToken::Token {
                denom: UncheckedDenom::Native("ujuno".to_string()),
            },
            amount: Uint128::new(10),
            refund_policy: DepositRefundPolicy::Never,
        }),
        true,
    );

    let config = get_config(
        &app,
        pre_propose_contract_info.address.clone(),
        pre_propose_contract_info.code_hash.clone(),
    );
    assert_eq!(
        config,
        Config {
            deposit_info: Some(CheckedDepositInfo {
                denom: cw_denom::CheckedDenom::Native("ujuno".to_string()),
                amount: Uint128::new(10),
                refund_policy: DepositRefundPolicy::Never
            }),
            open_proposal_submission: true,
        }
    );

    // Old proposal should still have same deposit info.
    let info = get_deposit_info(
        &app,
        pre_propose_contract_info.address.clone(),
        pre_propose_contract_info.code_hash.clone(),
        id,
    );
    assert_eq!(
        info,
        DepositInfoResponse {
            deposit_info: None,
            proposer: Addr::unchecked("ekez"),
        }
    );

    // New proposals should have the new deposit info.
    mint_natives(&mut app, "ekez", coins(10, "ujuno"));
    let new_pre_propose_id = make_pre_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "ekez",
        &coins(10, "ujuno"),
        Auth::ViewingKey {
            key: viewing_key_ekez.clone(),
            address: "ekez".into(),
        },
    );

    // Approver approves
    let new_id = approve_proposal(
        &mut app,
        pre_propose_contract_info.clone(),
        "approver",
        new_pre_propose_id,
    );

    let info = get_deposit_info(
        &app,
        pre_propose_contract_info.address.clone(),
        pre_propose_contract_info.code_hash.clone(),
        new_id,
    );
    assert_eq!(
        info,
        DepositInfoResponse {
            deposit_info: Some(CheckedDepositInfo {
                denom: cw_denom::CheckedDenom::Native("ujuno".to_string()),
                amount: Uint128::new(10),
                refund_policy: DepositRefundPolicy::Never
            }),
            proposer: Addr::unchecked("ekez"),
        }
    );

    // Both proposals should be allowed to complete.
    vote(
        &mut app,
        proposal_single_contract_info.clone(),
        "ekez",
        id,
        Vote::Yes,
        Auth::ViewingKey {
            key: viewing_key_ekez.clone(),
            address: "ekez".into(),
        },
    );
    vote(
        &mut app,
        proposal_single_contract_info.clone(),
        "ekez",
        new_id,
        Vote::Yes,
        Auth::ViewingKey {
            key: viewing_key_ekez.clone(),
            address: "ekez".into(),
        },
    );
    execute_proposal(
        &mut app,
        proposal_single_contract_info.clone(),
        "ekez",
        id,
        Auth::ViewingKey {
            key: viewing_key_ekez.clone(),
            address: "ekez".into(),
        },
    );
    execute_proposal(
        &mut app,
        proposal_single_contract_info.clone(),
        "ekez",
        new_id,
        Auth::ViewingKey {
            key: viewing_key_ekez.clone(),
            address: "ekez".into(),
        },
    );
    // Deposit should not have been refunded (never policy in use).
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(balance, Uint128::new(0));

    // Only the core module can update the config.
    let err = update_config_should_fail(
        &mut app,
        pre_propose_contract_info,
        proposal_single_contract_info.address.as_str(),
        None,
        true,
    );
    assert_eq!(err, PreProposeError::NotDao {});
}
