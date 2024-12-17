use crate::contract::*;
use cosmwasm_std::{
    coins, from_binary, to_binary, Addr, Binary, Coin, ContractInfo, Empty, Uint128,
};
use cw_denom::UncheckedDenom;
use cw_hooks::HookItem;
use dao_interface::state::{Admin, ModuleInstantiateInfo};
use dao_interface::state::{AnyContractInfo, ProposalModule};
use dao_pre_propose_base::{error::PreProposeError, msg::DepositInfoResponse, state::Config};
use dao_proposal_single as dps;
use dao_voting::{
    deposit::{CheckedDepositInfo, DepositRefundPolicy, DepositToken, UncheckedDepositInfo},
    pre_propose::{PreProposeInfo, ProposalCreationPolicy},
    status::Status,
    threshold::{PercentageThreshold, Threshold},
    voting::Vote,
};
use dao_voting_cw4::msg::GroupContract;
use dps::query::ProposalResponse;
use secret_multi_test::{
    App, BankSudo, Contract, ContractInstantiationInfo, ContractWrapper, Executor,
};
use secret_utils::Duration;
use shade_protocol::basic_staking::Auth;
use shade_protocol::utils::asset::RawContract;
use snip20_base::msg::{InitConfig, InitialBalance};
const CREATOR_ADDR: &str = "creator";

pub fn cw4_group_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cw4_group::contract::execute,
        cw4_group::contract::instantiate,
        cw4_group::contract::query,
    );
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

    let addr = app
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

    addr
}

fn cw_dao_proposal_single_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        dps::contract::execute,
        dps::contract::instantiate,
        dps::contract::query,
    )
    .with_migrate(dps::contract::migrate)
    .with_reply(dps::contract::reply);
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

fn get_default_proposal_module_instantiate(
    app: &mut App,
    deposit_info: Option<UncheckedDepositInfo>,
    open_proposal_submission: bool,
    query_auth: ContractInfo,
) -> dps::msg::InstantiateMsg {
    let pre_propose_info = app.store_code(cw_pre_propose_base_proposal_single());

    dps::msg::InstantiateMsg {
        threshold: Threshold::AbsolutePercentage {
            percentage: PercentageThreshold::Majority {},
        },
        max_voting_period: secret_utils::Duration::Time(86400),
        min_voting_period: None,
        only_members_execute: false,
        allow_revoting: false,
        pre_propose_info: PreProposeInfo::ModuleMayPropose {
            info: ModuleInstantiateInfo {
                code_id: pre_propose_info.code_id,
                code_hash: pre_propose_info.code_hash,
                msg: to_binary(&InstantiateMsg {
                    deposit_info,
                    open_proposal_submission,
                    extension: Empty::default(),
                })
                .unwrap(),
                admin: Some(Admin::CoreModule {}),
                funds: vec![],
                label: "baby's first pre-propose module".to_string(),
            },
        },
        close_proposal_on_execution_failure: false,
        veto: None,
        query_auth: Some(RawContract::new(
            &query_auth.address.into_string(),
            &query_auth.code_hash,
        )),
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

fn instantiate_query_auth(app: &mut App) -> ContractInfo {
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
        Addr::unchecked(CREATOR_ADDR),
        &msg,
        &[],
        "query_auth",
        None,
    )
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

struct DefaultTestSetup {
    core_contract_info: ContractInfo,
    proposal_single_info: ContractInfo,
    pre_propose_info: ContractInfo,
}
fn setup_default_test(
    app: &mut App,
    deposit_info: Option<UncheckedDepositInfo>,
    open_proposal_submission: bool,
) -> DefaultTestSetup {
    let dps_info = app.store_code(cw_dao_proposal_single_contract());
    let query_auth = instantiate_query_auth(app);
    let core_info = app.store_code(dao_dao_contract());

    let proposal_module_instantiate = get_default_proposal_module_instantiate(
        app,
        deposit_info,
        open_proposal_submission,
        query_auth,
    );

    let core_contract_info = instantiate_with_cw4_groups_governance(
        app,
        core_info,
        dps_info.code_id,
        dps_info.code_hash.clone(),
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
            &dps::msg::QueryMsg::ProposalCreationPolicy {},
        )
        .unwrap();

    let pre_propose = match proposal_creation_policy {
        ProposalCreationPolicy::Module { addr, code_hash } => (addr, code_hash),
        _ => panic!("expected a module for the proposal creation policy"),
    };

    // Make sure things were set up correctly.
    assert_eq!(
        AnyContractInfo {
            addr: proposal_single_address.clone(),
            code_hash: proposal_single_code_hash.clone()
        },
        get_proposal_module(app, pre_propose.0.clone(), pre_propose.1.clone())
    );
    assert_eq!(
        AnyContractInfo {
            addr: core_contract_info.address.clone(),
            code_hash: core_contract_info.code_hash.clone()
        },
        get_dao(app, pre_propose.0.clone(), pre_propose.1.clone())
    );

    DefaultTestSetup {
        core_contract_info,
        proposal_single_info: ContractInfo {
            address: proposal_single_address,
            code_hash: proposal_single_code_hash,
        },
        pre_propose_info: ContractInfo {
            address: pre_propose.0,
            code_hash: pre_propose.1,
        },
    }
}

fn make_proposal(
    app: &mut App,
    pre_propose_contract_info: ContractInfo,
    proposal_module_contract_info: ContractInfo,
    proposer: &str,
    auth: Auth,
    funds: &[Coin],
) -> u64 {
    app.execute_contract(
        Addr::unchecked(proposer),
        &pre_propose_contract_info,
        &ExecuteMsg::Propose {
            auth,
            msg: ProposeMessage::Propose {
                title: "title".to_string(),
                description: "description".to_string(),
                msgs: vec![],
            },
        },
        funds,
    )
    .unwrap();

    let id: u64 = app
        .wrap()
        .query_wasm_smart(
            &proposal_module_contract_info.code_hash.clone(),
            proposal_module_contract_info.address.clone(),
            &dps::msg::QueryMsg::NextProposalId {},
        )
        .unwrap();
    let id = id - 1;

    let proposal: ProposalResponse = app
        .wrap()
        .query_wasm_smart(
            proposal_module_contract_info.code_hash,
            proposal_module_contract_info.address,
            &dps::msg::QueryMsg::Proposal { proposal_id: id },
        )
        .unwrap();

    assert_eq!(proposal.proposal.proposer, Addr::unchecked(proposer));
    assert_eq!(proposal.proposal.title, "title".to_string());
    assert_eq!(proposal.proposal.description, "description".to_string());
    assert_eq!(proposal.proposal.msgs, vec![]);

    id
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

fn add_hook(
    app: &mut App,
    sender: &str,
    module_contract_info: ContractInfo,
    hook_addr: &str,
    hook_code_hash: &str,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &module_contract_info,
        &ExecuteMsg::AddProposalSubmittedHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash.to_string(),
        },
        &[],
    )
    .unwrap();
}

fn remove_hook(
    app: &mut App,
    sender: &str,
    module_contract_info: ContractInfo,
    hook_addr: &str,
    hook_code_hash: &str,
) {
    app.execute_contract(
        Addr::unchecked(sender),
        &module_contract_info,
        &ExecuteMsg::RemoveProposalSubmittedHook {
            address: hook_addr.to_string(),
            code_hash: hook_code_hash.to_string(),
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
        &dps::msg::ExecuteMsg::Vote {
            rationale: None,
            proposal_id: id,
            vote: position,
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
            &dps::msg::QueryMsg::Proposal { proposal_id: id },
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

fn query_hooks(app: &App, module_addr: Addr, module_code_hash: String) -> cw_hooks::HooksResponse {
    app.wrap()
        .query_wasm_smart(
            module_code_hash,
            module_addr,
            &QueryMsg::ProposalSubmittedHooks {},
        )
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

fn get_proposal_module(app: &App, module_addr: Addr, module_code_hash: String) -> AnyContractInfo {
    app.wrap()
        .query_wasm_smart(module_code_hash, module_addr, &QueryMsg::ProposalModule {})
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
    key: String,
) -> PreProposeError {
    app.execute_contract(
        Addr::unchecked(sender),
        &module_contract_info,
        &ExecuteMsg::Withdraw {
            denom,
            key: Some(key),
        },
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
        &dps::msg::ExecuteMsg::Close { proposal_id },
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
        &dps::msg::ExecuteMsg::Execute { auth, proposal_id },
        &[],
    )
    .unwrap();
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
) {
    let mut app = App::default();

    let DefaultTestSetup {
        core_contract_info,
        proposal_single_info,
        pre_propose_info,
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
    let id = make_proposal(
        &mut app,
        pre_propose_info,
        proposal_single_info.clone(),
        "ekez",
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".to_string(),
        },
        &coins(10, "ujuno"),
    );

    // Make sure it went away.
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(balance, Uint128::zero());

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
        proposal_single_info.clone(),
        "ekez",
        id,
        position,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".to_string(),
        },
    );
    assert_eq!(new_status, expected_status);

    // Close or execute the proposal to trigger a refund.
    trigger_refund(
        &mut app,
        proposal_single_info,
        "ekez",
        id,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".to_string(),
        },
    );

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
) {
    let mut app = App::default();

    let snip20_info = instantiate_snip20_base_default(&mut app);

    let DefaultTestSetup {
        core_contract_info,
        proposal_single_info,
        pre_propose_info,
    } = setup_default_test(
        &mut app,
        Some(UncheckedDepositInfo {
            denom: DepositToken::Token {
                denom: UncheckedDenom::Snip20(
                    snip20_info.address.clone().to_string(),
                    snip20_info.code_hash.clone(),
                ),
            },
            amount: Uint128::new(10),
            refund_policy,
        }),
        false,
    );

    increase_allowance(
        &mut app,
        "ekez",
        &pre_propose_info.address.clone(),
        snip20_info.clone(),
        Uint128::new(10),
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
    let viewing_key_token = create_viewing_key_snip20(&mut app, snip20_info.clone(), "ekez");
    let viewing_key_token_dao = create_viewing_key_snip20(
        &mut app,
        snip20_info.clone(),
        core_contract_info.address.clone().as_str(),
    );
    let id = make_proposal(
        &mut app,
        pre_propose_info.clone(),
        proposal_single_info.clone(),
        "ekez",
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".to_string(),
        },
        &[],
    );

    // Make sure it went await.
    let balance = get_balance_snip20(
        &app,
        snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        "ekez",
        viewing_key_token.clone(),
    );
    assert_eq!(balance, Uint128::zero());

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
        proposal_single_info.clone(),
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
        proposal_single_info.clone(),
        "ekez",
        id,
        Auth::ViewingKey {
            key: viewing_key,
            address: "ekez".to_string(),
        },
    );

    let (dao_expected, proposer_expected) = match receiver {
        RefundReceiver::Proposer => (0, 10),
        RefundReceiver::Dao => (10, 0),
    };

    let proposer_balance = get_balance_snip20(
        &app,
        snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        "ekez",
        viewing_key_token,
    );
    let dao_balance = get_balance_snip20(
        &app,
        &snip20_info.address,
        snip20_info.code_hash,
        core_contract_info.address,
        viewing_key_token_dao,
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
    )
}
#[test]
fn test_snip20_failed_always_refund() {
    test_snip20_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::Always,
        RefundReceiver::Proposer,
    )
}

#[test]
fn test_native_passed_always_refund() {
    test_native_permutation(
        EndStatus::Passed,
        DepositRefundPolicy::Always,
        RefundReceiver::Proposer,
    )
}

#[test]
fn test_snip20_passed_always_refund() {
    test_snip20_permutation(
        EndStatus::Passed,
        DepositRefundPolicy::Always,
        RefundReceiver::Proposer,
    )
}

#[test]
fn test_native_passed_never_refund() {
    test_native_permutation(
        EndStatus::Passed,
        DepositRefundPolicy::Never,
        RefundReceiver::Dao,
    )
}
#[test]
fn test_snip20_passed_never_refund() {
    test_snip20_permutation(
        EndStatus::Passed,
        DepositRefundPolicy::Never,
        RefundReceiver::Dao,
    )
}

#[test]
fn test_native_failed_never_refund() {
    test_native_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::Never,
        RefundReceiver::Dao,
    )
}
#[test]
fn test_snip20_failed_never_refund() {
    test_snip20_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::Never,
        RefundReceiver::Dao,
    )
}

#[test]
fn test_native_passed_passed_refund() {
    test_native_permutation(
        EndStatus::Passed,
        DepositRefundPolicy::OnlyPassed,
        RefundReceiver::Proposer,
    )
}
#[test]
fn test_snip20_passed_passed_refund() {
    test_snip20_permutation(
        EndStatus::Passed,
        DepositRefundPolicy::OnlyPassed,
        RefundReceiver::Proposer,
    )
}

#[test]
fn test_native_failed_passed_refund() {
    test_native_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::OnlyPassed,
        RefundReceiver::Dao,
    )
}
#[test]
fn test_snip20_failed_passed_refund() {
    test_snip20_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::OnlyPassed,
        RefundReceiver::Dao,
    )
}

// See: <https://github.com/DA0-DA0/dao-contracts/pull/465#discussion_r960092321>
#[test]
fn test_multiple_open_proposals() {
    let mut app = App::default();

    let DefaultTestSetup {
        core_contract_info,
        proposal_single_info,
        pre_propose_info,
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
    let first_id = make_proposal(
        &mut app,
        pre_propose_info.clone(),
        proposal_single_info.clone(),
        "ekez",
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
        &coins(10, "ujuno"),
    );
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(10, balance.u128());

    let second_id = make_proposal(
        &mut app,
        pre_propose_info.clone(),
        proposal_single_info.clone(),
        "ekez",
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
        &coins(10, "ujuno"),
    );
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(0, balance.u128());

    // Finish up the first proposal.
    let new_status = vote(
        &mut app,
        proposal_single_info.clone(),
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
        proposal_single_info.clone(),
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
        proposal_single_info.clone(),
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

    close_proposal(&mut app, proposal_single_info, "ekez", second_id);

    // All deposits have been refunded.
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(20, balance.u128());
}

#[test]
fn test_permissions() {
    let mut app = App::default();

    let DefaultTestSetup {
        core_contract_info,
        proposal_single_info: _,
        pre_propose_info,
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
            core_contract_info.address.clone(),
            &pre_propose_info.clone(),
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
            &pre_propose_info,
            &ExecuteMsg::Propose {
                auth: Auth::ViewingKey {
                    key: viewing_key,
                    address: "nonmember".into(),
                },
                msg: ProposeMessage::Propose {
                    title: "I would like to join the DAO".to_string(),
                    description: "though, I am currently not a member.".to_string(),
                    msgs: vec![],
                },
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, PreProposeError::NotMember {})
}

#[test]
fn test_propose_open_proposal_submission() {
    let mut app = App::default();
    let DefaultTestSetup {
        core_contract_info,
        proposal_single_info,
        pre_propose_info,
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
    let id = make_proposal(
        &mut app,
        pre_propose_info,
        proposal_single_info.clone(),
        "nonmember",
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "nonmember".into(),
        },
        &coins(10, "ujuno"),
    );
    // Member votes.
    let new_status = vote(
        &mut app,
        proposal_single_info,
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
        proposal_single_info,
        pre_propose_info,
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
    let id = make_proposal(
        &mut app,
        pre_propose_info,
        proposal_single_info.clone(),
        "nonmember",
        Auth::ViewingKey {
            key: viewing_key,
            address: "nonmember".into(),
        },
        &[],
    );
    // Member votes.
    let new_status = vote(
        &mut app,
        proposal_single_info,
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
        proposal_single_info,
        pre_propose_info,
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
            &pre_propose_info.clone(),
            &ExecuteMsg::Propose {
                auth: Auth::ViewingKey {
                    key: viewing_key,
                    address: "nonmember".into(),
                },
                msg: ProposeMessage::Propose {
                    title: "I would like to join the DAO".to_string(),
                    description: "though, I am currently not a member.".to_string(),
                    msgs: vec![],
                },
            },
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, PreProposeError::NotMember {});

    let id = make_proposal(
        &mut app,
        pre_propose_info,
        proposal_single_info.clone(),
        "ekez",
        Auth::ViewingKey {
            key: viewing_key_ekez.clone(),
            address: "ekez".into(),
        },
        &[],
    );
    let new_status = vote(
        &mut app,
        proposal_single_info,
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
fn test_execute_extension_does_nothing() {
    let mut app = App::default();
    let DefaultTestSetup {
        core_contract_info: _,
        proposal_single_info: _,
        pre_propose_info,
    } = setup_default_test(
        &mut app, None, false, // no open proposal submission.
    );

    let res = app
        .execute_contract(
            Addr::unchecked("ekez"),
            &pre_propose_info,
            &ExecuteMsg::Extension {
                msg: Empty::default(),
            },
            &[],
        )
        .unwrap();

    // There should be one event which is the invocation of the contract.
    assert_eq!(res.events.len(), 1);
    assert_eq!(res.events[0].ty, "execute".to_string());
    assert_eq!(res.events[0].attributes.len(), 1);
    assert_eq!(
        res.events[0].attributes[0].key,
        "_contract_addr".to_string()
    )
}

#[test]
#[should_panic(expected = "invalid zero deposit. set the deposit to `None` to have no deposit")]
fn test_instantiate_with_zero_native_deposit() {
    let mut app = App::default();

    let dps_info = app.store_code(cw_dao_proposal_single_contract());
    let core_info = app.store_code(dao_dao_contract());

    let proposal_module_instantiate = {
        let pre_propose_info = app.store_code(cw_pre_propose_base_proposal_single());

        dps::msg::InstantiateMsg {
            threshold: Threshold::AbsolutePercentage {
                percentage: PercentageThreshold::Majority {},
            },
            max_voting_period: Duration::Time(86400),
            min_voting_period: None,
            only_members_execute: false,
            allow_revoting: false,
            pre_propose_info: PreProposeInfo::ModuleMayPropose {
                info: ModuleInstantiateInfo {
                    code_id: pre_propose_info.code_id,
                    code_hash: pre_propose_info.code_hash.clone(),
                    msg: to_binary(&InstantiateMsg {
                        deposit_info: Some(UncheckedDepositInfo {
                            denom: DepositToken::Token {
                                denom: UncheckedDenom::Native("ujuno".to_string()),
                            },
                            amount: Uint128::zero(),
                            refund_policy: DepositRefundPolicy::OnlyPassed,
                        }),
                        open_proposal_submission: false,
                        extension: Empty::default(),
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
        core_info,
        dps_info.code_id,
        dps_info.code_hash,
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

    let snip20_info = instantiate_snip20_base_default(&mut app);

    let dps_info = app.store_code(cw_dao_proposal_single_contract());
    let core_info = app.store_code(dao_dao_contract());

    let proposal_module_instantiate = {
        let pre_propose_info = app.store_code(cw_pre_propose_base_proposal_single());

        dps::msg::InstantiateMsg {
            threshold: Threshold::AbsolutePercentage {
                percentage: PercentageThreshold::Majority {},
            },
            max_voting_period: Duration::Time(86400),
            min_voting_period: None,
            only_members_execute: false,
            allow_revoting: false,
            pre_propose_info: PreProposeInfo::ModuleMayPropose {
                info: ModuleInstantiateInfo {
                    code_id: pre_propose_info.code_id,
                    code_hash: pre_propose_info.code_hash.clone(),
                    msg: to_binary(&InstantiateMsg {
                        deposit_info: Some(UncheckedDepositInfo {
                            denom: DepositToken::Token {
                                denom: UncheckedDenom::Snip20(
                                    snip20_info.address.clone().into_string(),
                                    snip20_info.code_hash.clone(),
                                ),
                            },
                            amount: Uint128::zero(),
                            refund_policy: DepositRefundPolicy::OnlyPassed,
                        }),
                        open_proposal_submission: false,
                        extension: Empty::default(),
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
        core_info,
        dps_info.code_id,
        dps_info.code_hash,
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
        proposal_single_info,
        pre_propose_info,
    } = setup_default_test(&mut app, None, false);

    let config = get_config(
        &app,
        pre_propose_info.address.clone(),
        pre_propose_info.code_hash.clone(),
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
    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.addr.clone(),
            code_hash: query_auth.code_hash.clone(),
        },
        "ekez",
    );

    let id = make_proposal(
        &mut app,
        pre_propose_info.clone(),
        proposal_single_info.clone(),
        "ekez",
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
        &[],
    );

    update_config(
        &mut app,
        pre_propose_info.clone(),
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
        pre_propose_info.address.clone(),
        pre_propose_info.code_hash.clone(),
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
        pre_propose_info.address.clone(),
        pre_propose_info.code_hash.clone(),
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
    let new_id = make_proposal(
        &mut app,
        pre_propose_info.clone(),
        proposal_single_info.clone(),
        "ekez",
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
        &coins(10, "ujuno"),
    );
    let info = get_deposit_info(
        &app,
        pre_propose_info.address.clone(),
        pre_propose_info.code_hash.clone(),
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
        proposal_single_info.clone(),
        "ekez",
        id,
        Vote::Yes,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
    );
    vote(
        &mut app,
        proposal_single_info.clone(),
        "ekez",
        new_id,
        Vote::Yes,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
    );
    execute_proposal(
        &mut app,
        proposal_single_info.clone(),
        "ekez",
        id,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
    );
    execute_proposal(
        &mut app,
        proposal_single_info.clone(),
        "ekez",
        new_id,
        Auth::ViewingKey {
            key: viewing_key,
            address: "ekez".into(),
        },
    );
    // Deposit should not have been refunded (never policy in use).
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(balance, Uint128::new(0));

    // Only the core module can update the config.
    let err = update_config_should_fail(
        &mut app,
        pre_propose_info,
        proposal_single_info.address.as_str(),
        None,
        true,
    );
    assert_eq!(err, PreProposeError::NotDao {});
}

#[test]
fn test_hook_management() {
    let app = &mut App::default();
    let DefaultTestSetup {
        core_contract_info,
        proposal_single_info: _,
        pre_propose_info,
    } = setup_default_test(app, None, true);

    add_hook(
        app,
        core_contract_info.address.clone().as_str(),
        pre_propose_info.clone(),
        "one",
        "one",
    );
    add_hook(
        app,
        core_contract_info.address.clone().as_str(),
        pre_propose_info.clone(),
        "two",
        "two",
    );

    remove_hook(
        app,
        core_contract_info.address.as_str(),
        pre_propose_info.clone(),
        "one",
        "one",
    );

    let hooks = query_hooks(app, pre_propose_info.address, pre_propose_info.code_hash).hooks;
    assert_eq!(
        hooks,
        vec![HookItem {
            addr: Addr::unchecked("two"),
            code_hash: "two".into()
        }]
    )
}
