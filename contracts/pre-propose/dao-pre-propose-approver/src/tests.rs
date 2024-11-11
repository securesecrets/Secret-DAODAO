use cosmwasm_std::{
    coins, from_binary, to_binary, Addr, Binary, Coin, ContractInfo, Empty, Uint128,
};
use cw_denom::UncheckedDenom;
use dao_voting_cw4::msg::GroupContract;
use dps::query::{ProposalListResponse, ProposalResponse};
use secret_cw2::ContractVersion;
use secret_multi_test::{
    App, BankSudo, Contract, ContractInstantiationInfo, ContractWrapper, Executor,
};

use dao_interface::state::{Admin, ModuleInstantiateInfo};
use dao_interface::state::{AnyContractInfo, ProposalModule};
use dao_pre_propose_approval_single::{
    msg::{
        ExecuteExt, ExecuteMsg, InstantiateExt, InstantiateMsg, ProposeMessage, QueryExt, QueryMsg,
    },
    state::Proposal,
};
use dao_pre_propose_base::{error::PreProposeError, msg::DepositInfoResponse, state::Config};
use dao_proposal_single as dps;
use dao_voting::{
    deposit::{CheckedDepositInfo, DepositRefundPolicy, DepositToken, UncheckedDepositInfo},
    pre_propose::{PreProposeInfo, ProposalCreationPolicy},
    status::Status,
    threshold::{PercentageThreshold, Threshold},
    voting::Vote,
};
use shade_protocol::basic_staking::Auth;
use shade_protocol::utils::asset::RawContract;
use snip20_reference_impl::msg::{InitConfig, InitialBalance};

use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::msg::InstantiateMsg as ApproverInstantiateMsg;
use crate::msg::{
    ExecuteExt as ApproverExecuteExt, ExecuteMsg as ApproverExecuteMsg,
    QueryExt as ApproverQueryExt, QueryMsg as ApproverQueryMsg,
};

// The approver dao contract is the 6th contract instantiated
const APPROVER: &str = "contract6";
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

pub fn snip721_base_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip721_reference_impl::contract::execute,
        snip721_reference_impl::contract::instantiate,
        snip721_reference_impl::contract::query,
    );
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
    let snip20_info = app.store_code(snip20_base_contract());
    let snip721_info = app.store_code(snip721_base_contract());
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
                dao_code_hash: core_info.code_hash.clone(),
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
        snip20_code_hash: snip20_info.code_hash,
        snip721_code_hash: snip721_info.code_hash,
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
    let contract = ContractWrapper::new(
        dao_pre_propose_approval_single::contract::execute,
        dao_pre_propose_approval_single::contract::instantiate,
        dao_pre_propose_approval_single::contract::query,
    );
    Box::new(contract)
}

fn snip20_base_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip20_reference_impl::contract::execute,
        snip20_reference_impl::contract::instantiate,
        snip20_reference_impl::contract::query,
    );
    Box::new(contract)
}
fn pre_propose_approver_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );
    Box::new(contract)
}

fn get_proposal_module_approval_single_instantiate(
    app: &mut App,
    deposit_info: Option<UncheckedDepositInfo>,
    open_proposal_submission: bool,
    proposal_module_code_hash: String,
    query_auth: ContractInfo,
    dao_code_hash: String,
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
                code_hash: pre_propose_info.code_hash.clone(),
                msg: to_binary(&InstantiateMsg {
                    deposit_info,
                    open_proposal_submission,
                    extension: InstantiateExt {
                        approver: APPROVER.to_string(),
                    },
                    proposal_module_code_hash,
                })
                .unwrap(),
                admin: Some(Admin::CoreModule {}),
                funds: vec![],
                label: "baby's first pre-propose module, needs supervision".to_string(),
            },
        },
        close_proposal_on_execution_failure: false,
        veto: None,
        dao_code_hash,
        query_auth: Some(RawContract::new(
            &query_auth.address.into_string(),
            &query_auth.code_hash,
        )),
    }
}

fn get_proposal_module_approver_instantiate(
    app: &mut App,
    _deposit_info: Option<UncheckedDepositInfo>,
    _open_proposal_submission: bool,
    pre_propose_approval_contract_info: ContractInfo,
    proposal_module_code_hash: String,
    query_auth: ContractInfo,
    dao_code_hash: String,
) -> dps::msg::InstantiateMsg {
    let pre_propose_info = app.store_code(pre_propose_approver_contract());

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
                msg: to_binary(&ApproverInstantiateMsg {
                    pre_propose_approval_contract: pre_propose_approval_contract_info
                        .address
                        .into(),
                    pre_propose_approval_contract_code_hash: pre_propose_approval_contract_info
                        .code_hash,
                    proposal_module_code_hash,
                })
                .unwrap(),
                admin: Some(Admin::CoreModule {}),
                funds: vec![],
                label: "approver module".to_string(),
            },
        },
        close_proposal_on_execution_failure: false,
        veto: None,
        dao_code_hash,
        query_auth: Some(RawContract::new(
            &query_auth.address.into_string(),
            &query_auth.code_hash,
        )),
    }
}

fn instantiate_snip20_base_default(app: &mut App) -> ContractInfo {
    let snip20_info = app.store_code(snip20_base_contract());
    let snip20_instantiate = snip20_reference_impl::msg::InstantiateMsg {
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
    let msg = snip20_reference_impl::msg::ExecuteMsg::CreateViewingKey {
        entropy: "entropy".to_string(),
        padding: None,
    };
    let res = app
        .execute_contract(Addr::unchecked(addr), &contract_info, &msg, &[])
        .unwrap();
    let mut viewing_key = String::new();
    let data: snip20_reference_impl::msg::ExecuteAnswer = from_binary(&res.data.unwrap()).unwrap();
    if let snip20_reference_impl::msg::ExecuteAnswer::CreateViewingKey { key } = data {
        viewing_key = key;
    };
    viewing_key
}

struct DefaultTestSetup {
    core_contract_info: ContractInfo,
    proposal_single_contract_info: ContractInfo,
    pre_propose_contract_info: ContractInfo,
    approver_core_contract_info: ContractInfo,
    pre_propose_approver: ContractInfo,
    proposal_single_approver_contract_info: ContractInfo,
}

fn setup_default_test(
    app: &mut App,
    deposit_info: Option<UncheckedDepositInfo>,
    open_proposal_submission: bool,
) -> DefaultTestSetup {
    let dps_info = app.store_code(cw_dao_proposal_single_contract());
    let query_auth = instantiate_query_auth(app);
    let core_info = app.store_code(dao_dao_contract());

    // Instantiate SubDAO with pre-propose-approval-single
    let proposal_module_instantiate = get_proposal_module_approval_single_instantiate(
        app,
        deposit_info.clone(),
        open_proposal_submission,
        dps_info.code_hash.clone(),
        query_auth.clone(),
        core_info.code_hash.clone(),
    );
    let core_contract_info = instantiate_with_cw4_groups_governance(
        app,
        core_info.clone(),
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

    // Make sure things were set up correctly.
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

    // Instantiate SubDAO with pre-propose-approver
    let proposal_module_instantiate = get_proposal_module_approver_instantiate(
        app,
        deposit_info,
        open_proposal_submission,
        ContractInfo {
            address: pre_propose.0.clone(),
            code_hash: pre_propose.1.clone(),
        },
        dps_info.code_hash.clone(),
        query_auth.clone(),
        core_contract_info.code_hash.clone(),
    );

    let approver_core_contract_info = instantiate_with_cw4_groups_governance(
        app,
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
    let proposal_modules: Vec<ProposalModule> = app
        .wrap()
        .query_wasm_smart(
            approver_core_contract_info.code_hash.clone(),
            approver_core_contract_info.address.clone(),
            &dao_interface::msg::QueryMsg::ProposalModules {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();

    // Make sure things were set up correctly.
    assert_eq!(proposal_modules.len(), 1);
    let proposal_single_approver_addr =
        proposal_modules.clone().into_iter().next().unwrap().address;
    let proposal_single_approver_code_hash = proposal_modules
        .clone()
        .into_iter()
        .next()
        .unwrap()
        .code_hash;

    let proposal_creation_policy = app
        .wrap()
        .query_wasm_smart(
            proposal_single_approver_code_hash.clone(),
            proposal_single_approver_code_hash.clone(),
            &dps::msg::QueryMsg::ProposalCreationPolicy {},
        )
        .unwrap();
    let pre_propose_approver = match proposal_creation_policy {
        ProposalCreationPolicy::Module { addr, code_hash } => (addr, code_hash),
        _ => panic!("expected a module for the proposal creation policy"),
    };
    assert_eq!(
        AnyContractInfo {
            addr: proposal_single_approver_addr.clone(),
            code_hash: proposal_single_approver_code_hash.clone(),
        },
        get_proposal_module(
            app,
            pre_propose_approver.0.clone(),
            pre_propose_approver.1.clone()
        )
    );
    assert_eq!(
        AnyContractInfo {
            addr: approver_core_contract_info.address.clone(),
            code_hash: approver_core_contract_info.code_hash.clone(),
        },
        get_dao(
            app,
            pre_propose_approver.0.clone(),
            pre_propose_approver.1.clone()
        )
    );

    DefaultTestSetup {
        core_contract_info,
        proposal_single_contract_info: ContractInfo {
            address: proposal_single_address,
            code_hash: proposal_single_code_hash,
        },
        pre_propose_contract_info: ContractInfo {
            address: pre_propose.0,
            code_hash: pre_propose.1,
        },
        approver_core_contract_info,
        proposal_single_approver_contract_info: ContractInfo {
            address: proposal_single_approver_addr,
            code_hash: proposal_single_approver_code_hash,
        },
        pre_propose_approver: ContractInfo {
            address: pre_propose_approver.0,
            code_hash: pre_propose_approver.1,
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

    // Query for pending proposal and return latest id
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

    // Return last item in list, id is first element of tuple
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
        &snip20_reference_impl::msg::ExecuteMsg::IncreaseAllowance {
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
    let msg = snip20_reference_impl::msg::QueryMsg::Balance {
        address: address.into(),
        key: key.into(),
    };
    let result: snip20_reference_impl::msg::QueryAnswer = app
        .wrap()
        .query_wasm_smart(code_hash, contract_addr, &msg)
        .unwrap();
    let mut balance = Uint128::zero();
    if let snip20_reference_impl::msg::QueryAnswer::Balance { amount } = result {
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
            &dps::msg::QueryMsg::Proposal { proposal_id: id },
        )
        .unwrap();

    proposal.proposal.status
}

fn get_config(app: &App, module_addr: Addr, module_code_hash: String) -> Config {
    app.wrap()
        .query_wasm_smart(module_addr, module_code_hash, &QueryMsg::Config {})
        .unwrap()
}

fn get_dao(app: &App, module_addr: Addr, module_code_hash: String) -> AnyContractInfo {
    app.wrap()
        .query_wasm_smart(module_code_hash, module_addr, &QueryMsg::Dao {})
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

fn get_proposals(app: &App, module_addr: Addr, module_code_hash: String) -> ProposalListResponse {
    app.wrap()
        .query_wasm_smart(
            module_code_hash,
            module_addr,
            &dps::msg::QueryMsg::ListProposals {
                start_after: None,
                limit: None,
            },
        )
        .unwrap()
}

fn get_latest_proposal_id(app: &App, module_contract_info: ContractInfo) -> u64 {
    // Check prop was created in the main DAO
    let props: ProposalListResponse = app
        .wrap()
        .query_wasm_smart(
            module_contract_info.code_hash,
            module_contract_info.address,
            &dps::msg::QueryMsg::ListProposals {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
    props.proposals[props.proposals.len() - 1].id
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

fn withdraw(
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

fn withdraw_should_fail(
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

fn approve_proposal(
    app: &mut App,
    module_contract_info: ContractInfo,
    sender: &str,
    proposal_id: u64,
    auth: Auth,
) {
    // Approver votes on prop
    vote(
        app,
        module_contract_info.clone(),
        sender,
        proposal_id,
        Vote::Yes,
        auth.clone(),
    );
    // Approver executes prop
    execute_proposal(app, module_contract_info, sender, proposal_id, auth);
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

    // Need to instantiate this so contract addresses match with cw20 test cases
    let _ = instantiate_snip20_base_default(&mut app);

    let DefaultTestSetup {
        core_contract_info,
        proposal_single_contract_info,
        pre_propose_contract_info,
        approver_core_contract_info,
        pre_propose_approver,
        proposal_single_approver_contract_info,
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
    let _pre_propose_id = make_pre_proposal(
        &mut app,
        pre_propose_contract_info,
        "ekez",
        &coins(10, "ujuno"),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: "ekez".into(),
        },
    );

    // Check no props created on main DAO yet
    let props = get_proposals(
        &app,
        proposal_single_contract_info.address.clone(),
        proposal_single_contract_info.code_hash.clone(),
    );
    assert_eq!(props.proposals.len(), 0);

    // Make sure it went away.
    let balance = get_balance_native(&app, "ekez", "ujuno");
    assert_eq!(balance, Uint128::zero());

    // Approver approves or rejects proposal
    match approval_status {
        ApprovalStatus::Approved => {
            // Get approver proposal id
            let id = get_latest_proposal_id(&app, proposal_single_approver_contract_info.clone());

            // Approver votes on prop
            vote(
                &mut app,
                proposal_single_approver_contract_info.clone(),
                "ekez",
                id,
                Vote::Yes,
                Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: "ekez".into(),
                },
            );
            // Approver executes prop
            execute_proposal(
                &mut app,
                proposal_single_approver_contract_info,
                "ekez",
                id,
                Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: "ekez".into(),
                },
            );

            // Check prop was created in the main DAO
            let id = get_latest_proposal_id(&app, proposal_single_contract_info.clone());
            let props = get_proposals(
                &app,
                proposal_single_contract_info.address.clone(),
                proposal_single_contract_info.code_hash.clone(),
            );
            assert_eq!(props.proposals.len(), 1);

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
            // Approver votes on prop
            // No proposal is created so there is no voting
            vote(
                &mut app,
                proposal_single_approver_contract_info.clone(),
                "ekez",
                1,
                Vote::No,
                Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: "ekez".into(),
                },
            );
            // Approver executes prop
            close_proposal(&mut app, proposal_single_approver_contract_info, "ekez", 1);

            // No prop created
            let props = get_proposals(
                &app,
                proposal_single_contract_info.address,
                proposal_single_contract_info.code_hash,
            );
            assert_eq!(props.proposals.len(), 0);
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

// fn test_cw20_permutation(
//     end_status: EndStatus,
//     refund_policy: DepositRefundPolicy,
//     receiver: RefundReceiver,
//     approval_status: ApprovalStatus,
// ) {
//     let mut app = App::default();

//     let cw20_address = instantiate_cw20_base_default(&mut app);

//     let DefaultTestSetup {
//         core_addr,
//         proposal_single,
//         pre_propose,
//         approver_core_addr: _,
//         proposal_single_approver,
//         pre_propose_approver: _,
//     } = setup_default_test(
//         &mut app,
//         Some(UncheckedDepositInfo {
//             denom: DepositToken::Token {
//                 denom: UncheckedDenom::Cw20(cw20_address.to_string()),
//             },
//             amount: Uint128::new(10),
//             refund_policy,
//         }),
//         false,
//     );

//     increase_allowance(
//         &mut app,
//         "ekez",
//         &pre_propose,
//         cw20_address.clone(),
//         Uint128::new(10),
//     );
//     let _pre_propose_id = make_pre_proposal(&mut app, pre_propose.clone(), "ekez", &[]);

//     // Check no props created on main DAO yet
//     let props = get_proposals(&app, proposal_single.clone());
//     assert_eq!(props.proposals.len(), 0);

//     // Make sure it went await.
//     let balance = get_balance_cw20(&app, cw20_address.clone(), "ekez");
//     assert_eq!(balance, Uint128::zero());

//     // Approver approves or rejects proposal
//     match approval_status {
//         ApprovalStatus::Approved => {
//             // Get approver proposal id
//             let id = get_latest_proposal_id(&app, proposal_single_approver.clone());

//             // Approver votes on prop
//             vote(
//                 &mut app,
//                 proposal_single_approver.clone(),
//                 "ekez",
//                 id,
//                 Vote::Yes,
//             );
//             // Approver executes prop
//             execute_proposal(&mut app, proposal_single_approver, "ekez", id);

//             // Check prop was created in the main DAO
//             let id = get_latest_proposal_id(&app, proposal_single.clone());
//             let props = get_proposals(&app, proposal_single.clone());
//             assert_eq!(props.proposals.len(), 1);

//             // Voting happens on newly created proposal
//             #[allow(clippy::type_complexity)]
//             let (position, expected_status, trigger_refund): (
//                 _,
//                 _,
//                 fn(&mut App, Addr, &str, u64) -> (),
//             ) = match end_status {
//                 EndStatus::Passed => (Vote::Yes, Status::Passed, execute_proposal),
//                 EndStatus::Failed => (Vote::No, Status::Rejected, close_proposal),
//             };
//             let new_status = vote(&mut app, proposal_single.clone(), "ekez", id, position);
//             assert_eq!(new_status, expected_status);

//             // Close or execute the proposal to trigger a refund.
//             trigger_refund(&mut app, proposal_single, "ekez", id);
//         }
//         ApprovalStatus::Rejected => {
//             // Approver votes on prop
//             // No proposal is created so there is no voting
//             vote(
//                 &mut app,
//                 proposal_single_approver.clone(),
//                 "ekez",
//                 1,
//                 Vote::No,
//             );
//             // Approver executes prop
//             close_proposal(&mut app, proposal_single_approver, "ekez", 1);

//             // No prop created
//             let props = get_proposals(&app, proposal_single);
//             assert_eq!(props.proposals.len(), 0);
//         }
//     };

//     let (dao_expected, proposer_expected) = match receiver {
//         RefundReceiver::Proposer => (0, 10),
//         RefundReceiver::Dao => (10, 0),
//     };

//     let proposer_balance = get_balance_cw20(&app, &cw20_address, "ekez");
//     let dao_balance = get_balance_cw20(&app, &cw20_address, core_addr);
//     assert_eq!(proposer_expected, proposer_balance.u128());
//     assert_eq!(dao_expected, dao_balance.u128())
// }

#[test]
fn test_native_failed_always_refund() {
    test_native_permutation(
        EndStatus::Failed,
        DepositRefundPolicy::Always,
        RefundReceiver::Proposer,
        ApprovalStatus::Approved,
    )
}

// #[test]
// fn test_native_rejected_always_refund() {
//     test_native_permutation(
//         EndStatus::Failed,
//         DepositRefundPolicy::Always,
//         RefundReceiver::Proposer,
//         ApprovalStatus::Rejected,
//     )
// }

// #[test]
// fn test_cw20_failed_always_refund() {
//     test_cw20_permutation(
//         EndStatus::Failed,
//         DepositRefundPolicy::Always,
//         RefundReceiver::Proposer,
//         ApprovalStatus::Approved,
//     )
// }

// #[test]
// fn test_cw20_rejected_always_refund() {
//     test_cw20_permutation(
//         EndStatus::Failed,
//         DepositRefundPolicy::Always,
//         RefundReceiver::Proposer,
//         ApprovalStatus::Rejected,
//     )
// }

// #[test]
// fn test_native_passed_always_refund() {
//     test_native_permutation(
//         EndStatus::Passed,
//         DepositRefundPolicy::Always,
//         RefundReceiver::Proposer,
//         ApprovalStatus::Approved,
//     )
// }

// #[test]
// fn test_cw20_passed_always_refund() {
//     test_cw20_permutation(
//         EndStatus::Passed,
//         DepositRefundPolicy::Always,
//         RefundReceiver::Proposer,
//         ApprovalStatus::Approved,
//     )
// }

// #[test]
// fn test_native_passed_never_refund() {
//     test_native_permutation(
//         EndStatus::Passed,
//         DepositRefundPolicy::Never,
//         RefundReceiver::Dao,
//         ApprovalStatus::Approved,
//     )
// }

// #[test]
// fn test_cw20_passed_never_refund() {
//     test_cw20_permutation(
//         EndStatus::Passed,
//         DepositRefundPolicy::Never,
//         RefundReceiver::Dao,
//         ApprovalStatus::Approved,
//     )
// }

// #[test]
// fn test_native_failed_never_refund() {
//     test_native_permutation(
//         EndStatus::Failed,
//         DepositRefundPolicy::Never,
//         RefundReceiver::Dao,
//         ApprovalStatus::Approved,
//     )
// }

// #[test]
// fn test_native_rejected_never_refund() {
//     test_native_permutation(
//         EndStatus::Failed,
//         DepositRefundPolicy::Never,
//         RefundReceiver::Dao,
//         ApprovalStatus::Rejected,
//     )
// }

// #[test]
// fn test_cw20_failed_never_refund() {
//     test_cw20_permutation(
//         EndStatus::Failed,
//         DepositRefundPolicy::Never,
//         RefundReceiver::Dao,
//         ApprovalStatus::Approved,
//     )
// }

// #[test]
// fn test_cw20_rejected_never_refund() {
//     test_cw20_permutation(
//         EndStatus::Failed,
//         DepositRefundPolicy::Never,
//         RefundReceiver::Dao,
//         ApprovalStatus::Rejected,
//     )
// }

// #[test]
// fn test_native_passed_passed_refund() {
//     test_native_permutation(
//         EndStatus::Passed,
//         DepositRefundPolicy::OnlyPassed,
//         RefundReceiver::Proposer,
//         ApprovalStatus::Approved,
//     )
// }

// #[test]
// fn test_cw20_passed_passed_refund() {
//     test_cw20_permutation(
//         EndStatus::Passed,
//         DepositRefundPolicy::OnlyPassed,
//         RefundReceiver::Proposer,
//         ApprovalStatus::Approved,
//     )
// }

// #[test]
// fn test_native_failed_passed_refund() {
//     test_native_permutation(
//         EndStatus::Failed,
//         DepositRefundPolicy::OnlyPassed,
//         RefundReceiver::Dao,
//         ApprovalStatus::Approved,
//     )
// }

// #[test]
// fn test_native_rejected_passed_refund() {
//     test_native_permutation(
//         EndStatus::Failed,
//         DepositRefundPolicy::OnlyPassed,
//         RefundReceiver::Dao,
//         ApprovalStatus::Rejected,
//     )
// }

// #[test]
// fn test_cw20_failed_passed_refund() {
//     test_cw20_permutation(
//         EndStatus::Failed,
//         DepositRefundPolicy::OnlyPassed,
//         RefundReceiver::Dao,
//         ApprovalStatus::Approved,
//     )
// }

// #[test]
// fn test_cw20_rejected_passed_refund() {
//     test_cw20_permutation(
//         EndStatus::Failed,
//         DepositRefundPolicy::OnlyPassed,
//         RefundReceiver::Dao,
//         ApprovalStatus::Rejected,
//     )
// }

// // See: <https://github.com/DA0-DA0/dao-contracts/pull/465#discussion_r960092321>
// #[test]
// fn test_multiple_open_proposals() {
//     let mut app = App::default();

//     // Need to instantiate this so contract addresses match with cw20 test cases
//     let _ = instantiate_cw20_base_default(&mut app);

//     let DefaultTestSetup {
//         core_addr: _,
//         proposal_single,
//         pre_propose,
//         approver_core_addr: _,
//         proposal_single_approver,
//         pre_propose_approver: _,
//     } = setup_default_test(
//         &mut app,
//         Some(UncheckedDepositInfo {
//             denom: DepositToken::Token {
//                 denom: UncheckedDenom::Native("ujuno".to_string()),
//             },
//             amount: Uint128::new(10),
//             refund_policy: DepositRefundPolicy::Always,
//         }),
//         false,
//     );

//     mint_natives(&mut app, "ekez", coins(20, "ujuno"));
//     let _first_pre_propose_id =
//         make_pre_proposal(&mut app, pre_propose.clone(), "ekez", &coins(10, "ujuno"));
//     let balance = get_balance_native(&app, "ekez", "ujuno");
//     assert_eq!(10, balance.u128());

//     // Approver DAO approves prop, balance remains the same
//     let approver_prop_id = get_latest_proposal_id(&app, proposal_single_approver.clone());
//     approve_proposal(
//         &mut app,
//         proposal_single_approver.clone(),
//         "ekez",
//         approver_prop_id,
//     );
//     let first_id = get_latest_proposal_id(&app, proposal_single.clone());
//     let balance = get_balance_native(&app, "ekez", "ujuno");
//     assert_eq!(10, balance.u128());

//     let _second_pre_propose_id =
//         make_pre_proposal(&mut app, pre_propose, "ekez", &coins(10, "ujuno"));
//     let balance = get_balance_native(&app, "ekez", "ujuno");
//     assert_eq!(0, balance.u128());

//     // Approver DAO votes to approves, balance remains the same
//     let approver_prop_id = get_latest_proposal_id(&app, proposal_single_approver.clone());
//     approve_proposal(&mut app, proposal_single_approver, "ekez", approver_prop_id);
//     let second_id = get_latest_proposal_id(&app, proposal_single.clone());
//     let balance = get_balance_native(&app, "ekez", "ujuno");
//     assert_eq!(0, balance.u128());

//     // Finish up the first proposal.
//     let new_status = vote(
//         &mut app,
//         proposal_single.clone(),
//         "ekez",
//         first_id,
//         Vote::Yes,
//     );
//     assert_eq!(Status::Passed, new_status);

//     // Still zero.
//     let balance = get_balance_native(&app, "ekez", "ujuno");
//     assert_eq!(0, balance.u128());

//     execute_proposal(&mut app, proposal_single.clone(), "ekez", first_id);

//     // First proposal refunded.
//     let balance = get_balance_native(&app, "ekez", "ujuno");
//     assert_eq!(10, balance.u128());

//     // Finish up the second proposal.
//     let new_status = vote(
//         &mut app,
//         proposal_single.clone(),
//         "ekez",
//         second_id,
//         Vote::No,
//     );
//     assert_eq!(Status::Rejected, new_status);

//     // Still zero.
//     let balance = get_balance_native(&app, "ekez", "ujuno");
//     assert_eq!(10, balance.u128());

//     close_proposal(&mut app, proposal_single, "ekez", second_id);

//     // All deposits have been refunded.
//     let balance = get_balance_native(&app, "ekez", "ujuno");
//     assert_eq!(20, balance.u128());
// }

// #[test]
// fn test_set_version() {
//     let mut app = App::default();

//     // Need to instantiate this so contract addresses match with cw20 test cases
//     let _ = instantiate_cw20_base_default(&mut app);

//     let DefaultTestSetup {
//         core_addr: _,
//         proposal_single: _,
//         pre_propose: _,
//         approver_core_addr: _,
//         proposal_single_approver: _,
//         pre_propose_approver,
//     } = setup_default_test(
//         &mut app,
//         Some(UncheckedDepositInfo {
//             denom: DepositToken::Token {
//                 denom: UncheckedDenom::Native("ujuno".to_string()),
//             },
//             amount: Uint128::new(10),
//             refund_policy: DepositRefundPolicy::Always,
//         }),
//         false,
//     );

//     let info: ContractVersion = from_binary(
//         app.wrap()
//             .query_wasm_raw(pre_propose_approver, "contract_info".as_bytes())
//             .unwrap()
//             .unwrap(),
//     )
//     .unwrap();
//     assert_eq!(
//         ContractVersion {
//             contract: CONTRACT_NAME.to_string(),
//             version: CONTRACT_VERSION.to_string()
//         },
//         info
//     )
// }

// #[test]
// fn test_permissions() {
//     let mut app = App::default();

//     // Need to instantiate this so contract addresses match with cw20 test cases
//     let _ = instantiate_cw20_base_default(&mut app);

//     let DefaultTestSetup {
//         core_addr,
//         proposal_single: _,
//         pre_propose,
//         approver_core_addr: _,
//         proposal_single_approver: _,
//         pre_propose_approver: _,
//     } = setup_default_test(
//         &mut app,
//         Some(UncheckedDepositInfo {
//             denom: DepositToken::Token {
//                 denom: UncheckedDenom::Native("ujuno".to_string()),
//             },
//             amount: Uint128::new(10),
//             refund_policy: DepositRefundPolicy::Always,
//         }),
//         false, // no open proposal submission.
//     );

//     let err: PreProposeError = app
//         .execute_contract(
//             core_addr,
//             pre_propose.clone(),
//             &ExecuteMsg::ProposalCompletedHook {
//                 proposal_id: 1,
//                 new_status: Status::Closed,
//             },
//             &[],
//         )
//         .unwrap_err()
//         .downcast()
//         .unwrap();
//     assert_eq!(err, PreProposeError::NotModule {});

//     // Non-members may not propose when open_propose_submission is
//     // disabled.
//     let err: PreProposeError = app
//         .execute_contract(
//             Addr::unchecked("nonmember"),
//             pre_propose,
//             &ExecuteMsg::Propose {
//                 msg: ProposeMessage::Propose {
//                     title: "I would like to join the DAO".to_string(),
//                     description: "though, I am currently not a member.".to_string(),
//                     msgs: vec![],
//                 },
//             },
//             &[],
//         )
//         .unwrap_err()
//         .downcast()
//         .unwrap();
//     assert_eq!(err, PreProposeError::NotMember {});
// }

// #[test]
// fn test_approval_and_rejection_permissions() {
//     let mut app = App::default();

//     // Need to instantiate this so contract addresses match with cw20 test cases
//     let _ = instantiate_cw20_base_default(&mut app);

//     let DefaultTestSetup {
//         core_addr: _,
//         proposal_single: _,
//         pre_propose,
//         approver_core_addr: _,
//         proposal_single_approver: _,
//         pre_propose_approver: _,
//     } = setup_default_test(
//         &mut app,
//         Some(UncheckedDepositInfo {
//             denom: DepositToken::Token {
//                 denom: UncheckedDenom::Native("ujuno".to_string()),
//             },
//             amount: Uint128::new(10),
//             refund_policy: DepositRefundPolicy::Always,
//         }),
//         true, // yes, open proposal submission.
//     );

//     // Non-member proposes.
//     mint_natives(&mut app, "nonmember", coins(10, "ujuno"));
//     let pre_propose_id = make_pre_proposal(
//         &mut app,
//         pre_propose.clone(),
//         "nonmember",
//         &coins(10, "ujuno"),
//     );

//     // Only approver can propose
//     let err: PreProposeError = app
//         .execute_contract(
//             Addr::unchecked("nonmember"),
//             pre_propose.clone(),
//             &ExecuteMsg::Extension {
//                 msg: ExecuteExt::Approve { id: pre_propose_id },
//             },
//             &[],
//         )
//         .unwrap_err()
//         .downcast()
//         .unwrap();
//     assert_eq!(err, PreProposeError::Unauthorized {});

//     // Only approver can propose
//     let err: PreProposeError = app
//         .execute_contract(
//             Addr::unchecked("nonmember"),
//             pre_propose,
//             &ExecuteMsg::Extension {
//                 msg: ExecuteExt::Reject { id: pre_propose_id },
//             },
//             &[],
//         )
//         .unwrap_err()
//         .downcast()
//         .unwrap();
//     assert_eq!(err, PreProposeError::Unauthorized {});
// }

// #[test]
// fn test_propose_open_proposal_submission() {
//     let mut app = App::default();

//     // Need to instantiate this so contract addresses match with cw20 test cases
//     let _ = instantiate_cw20_base_default(&mut app);

//     let DefaultTestSetup {
//         core_addr: _,
//         proposal_single,
//         pre_propose,
//         approver_core_addr: _,
//         proposal_single_approver,
//         pre_propose_approver,
//     } = setup_default_test(
//         &mut app,
//         Some(UncheckedDepositInfo {
//             denom: DepositToken::Token {
//                 denom: UncheckedDenom::Native("ujuno".to_string()),
//             },
//             amount: Uint128::new(10),
//             refund_policy: DepositRefundPolicy::Always,
//         }),
//         true, // yes, open proposal submission.
//     );

//     // Non-member proposes.
//     mint_natives(&mut app, "nonmember", coins(10, "ujuno"));
//     let pre_propose_id = make_pre_proposal(&mut app, pre_propose, "nonmember", &coins(10, "ujuno"));

//     let approver_prop_id = get_latest_proposal_id(&app, proposal_single_approver.clone());
//     let pre_propose_id_from_proposal: u64 = app
//         .wrap()
//         .query_wasm_smart(
//             pre_propose_approver.clone(),
//             &ApproverQueryMsg::QueryExtension {
//                 msg: ApproverQueryExt::PreProposeApprovalIdForApproverProposalId {
//                     id: approver_prop_id,
//                 },
//             },
//         )
//         .unwrap();
//     assert_eq!(pre_propose_id_from_proposal, pre_propose_id);

//     let proposal_id_from_pre_propose: u64 = app
//         .wrap()
//         .query_wasm_smart(
//             pre_propose_approver.clone(),
//             &ApproverQueryMsg::QueryExtension {
//                 msg: ApproverQueryExt::ApproverProposalIdForPreProposeApprovalId {
//                     id: pre_propose_id,
//                 },
//             },
//         )
//         .unwrap();
//     assert_eq!(proposal_id_from_pre_propose, approver_prop_id);

//     // Approver DAO votes to approves
//     approve_proposal(&mut app, proposal_single_approver, "ekez", approver_prop_id);
//     let id = get_latest_proposal_id(&app, proposal_single.clone());

//     // Member votes.
//     let new_status = vote(&mut app, proposal_single, "ekez", id, Vote::Yes);
//     assert_eq!(Status::Passed, new_status)
// }

// #[test]
// fn test_update_config() {
//     let mut app = App::default();

//     // Need to instantiate this so contract addresses match with cw20 test cases
//     let _ = instantiate_cw20_base_default(&mut app);

//     let DefaultTestSetup {
//         core_addr,
//         proposal_single,
//         pre_propose,
//         approver_core_addr: _,
//         proposal_single_approver,
//         pre_propose_approver: _,
//     } = setup_default_test(&mut app, None, false);

//     let config = get_config(&app, pre_propose.clone());
//     assert_eq!(
//         config,
//         Config {
//             deposit_info: None,
//             open_proposal_submission: false
//         }
//     );

//     let _pre_propose_id = make_pre_proposal(&mut app, pre_propose.clone(), "ekez", &[]);

//     // Approver DAO votes to approves
//     let approver_prop_id = get_latest_proposal_id(&app, proposal_single_approver.clone());
//     approve_proposal(
//         &mut app,
//         proposal_single_approver.clone(),
//         "ekez",
//         approver_prop_id,
//     );
//     let id = get_latest_proposal_id(&app, proposal_single.clone());

//     update_config(
//         &mut app,
//         pre_propose.clone(),
//         core_addr.as_str(),
//         Some(UncheckedDepositInfo {
//             denom: DepositToken::Token {
//                 denom: UncheckedDenom::Native("ujuno".to_string()),
//             },
//             amount: Uint128::new(10),
//             refund_policy: DepositRefundPolicy::Never,
//         }),
//         true,
//     );

//     let config = get_config(&app, pre_propose.clone());
//     assert_eq!(
//         config,
//         Config {
//             deposit_info: Some(CheckedDepositInfo {
//                 denom: cw_denom::CheckedDenom::Native("ujuno".to_string()),
//                 amount: Uint128::new(10),
//                 refund_policy: DepositRefundPolicy::Never
//             }),
//             open_proposal_submission: true,
//         }
//     );

//     // Old proposal should still have same deposit info.
//     let info = get_deposit_info(&app, pre_propose.clone(), id);
//     assert_eq!(
//         info,
//         DepositInfoResponse {
//             deposit_info: None,
//             proposer: Addr::unchecked("ekez"),
//         }
//     );

//     // New proposals should have the new deposit info.
//     mint_natives(&mut app, "ekez", coins(10, "ujuno"));
//     let _new_pre_propose_id =
//         make_pre_proposal(&mut app, pre_propose.clone(), "ekez", &coins(10, "ujuno"));

//     // Approver DAO votes to approve prop
//     let approver_prop_id = get_latest_proposal_id(&app, proposal_single_approver.clone());
//     approve_proposal(
//         &mut app,
//         proposal_single_approver.clone(),
//         "ekez",
//         approver_prop_id,
//     );
//     let new_id = get_latest_proposal_id(&app, proposal_single_approver);

//     let info = get_deposit_info(&app, pre_propose.clone(), new_id);
//     assert_eq!(
//         info,
//         DepositInfoResponse {
//             deposit_info: Some(CheckedDepositInfo {
//                 denom: cw_denom::CheckedDenom::Native("ujuno".to_string()),
//                 amount: Uint128::new(10),
//                 refund_policy: DepositRefundPolicy::Never
//             }),
//             proposer: Addr::unchecked("ekez"),
//         }
//     );

//     // Both proposals should be allowed to complete.
//     vote(&mut app, proposal_single.clone(), "ekez", id, Vote::Yes);
//     vote(&mut app, proposal_single.clone(), "ekez", new_id, Vote::Yes);
//     execute_proposal(&mut app, proposal_single.clone(), "ekez", id);
//     execute_proposal(&mut app, proposal_single.clone(), "ekez", new_id);
//     // Deposit should not have been refunded (never policy in use).
//     let balance = get_balance_native(&app, "ekez", "ujuno");
//     assert_eq!(balance, Uint128::new(0));

//     // Only the core module can update the config.
//     let err =
//         update_config_should_fail(&mut app, pre_propose, proposal_single.as_str(), None, true);
//     assert_eq!(err, PreProposeError::NotDao {});
// }

// #[test]
// fn test_withdraw() {
//     let mut app = App::default();

//     // Need to instantiate this so contract addresses match with cw20 test cases
//     let _ = instantiate_cw20_base_default(&mut app);

//     let DefaultTestSetup {
//         core_addr,
//         proposal_single,
//         pre_propose,
//         approver_core_addr: _,
//         proposal_single_approver,
//         pre_propose_approver: _,
//     } = setup_default_test(&mut app, None, false);

//     let err = withdraw_should_fail(
//         &mut app,
//         pre_propose.clone(),
//         proposal_single.as_str(),
//         Some(UncheckedDenom::Native("ujuno".to_string())),
//     );
//     assert_eq!(err, PreProposeError::NotDao {});

//     let err = withdraw_should_fail(
//         &mut app,
//         pre_propose.clone(),
//         core_addr.as_str(),
//         Some(UncheckedDenom::Native("ujuno".to_string())),
//     );
//     assert_eq!(err, PreProposeError::NothingToWithdraw {});

//     let err = withdraw_should_fail(&mut app, pre_propose.clone(), core_addr.as_str(), None);
//     assert_eq!(err, PreProposeError::NoWithdrawalDenom {});

//     // Turn on native deposits.
//     update_config(
//         &mut app,
//         pre_propose.clone(),
//         core_addr.as_str(),
//         Some(UncheckedDepositInfo {
//             denom: DepositToken::Token {
//                 denom: UncheckedDenom::Native("ujuno".to_string()),
//             },
//             amount: Uint128::new(10),
//             refund_policy: DepositRefundPolicy::Always,
//         }),
//         false,
//     );

//     // Withdraw with no specified denom - should fall back to the one
//     // in the config.
//     mint_natives(&mut app, pre_propose.as_str(), coins(10, "ujuno"));
//     withdraw(&mut app, pre_propose.clone(), core_addr.as_str(), None);
//     let balance = get_balance_native(&app, core_addr.as_str(), "ujuno");
//     assert_eq!(balance, Uint128::new(10));

//     // Withdraw again, this time specifying a native denomination.
//     mint_natives(&mut app, pre_propose.as_str(), coins(10, "ujuno"));
//     withdraw(
//         &mut app,
//         pre_propose.clone(),
//         core_addr.as_str(),
//         Some(UncheckedDenom::Native("ujuno".to_string())),
//     );
//     let balance = get_balance_native(&app, core_addr.as_str(), "ujuno");
//     assert_eq!(balance, Uint128::new(20));

//     // Make a proposal with the native tokens to put some in the system.
//     mint_natives(&mut app, "ekez", coins(10, "ujuno"));
//     let _native_pre_propose_id =
//         make_pre_proposal(&mut app, pre_propose.clone(), "ekez", &coins(10, "ujuno"));

//     // Approver DAO votes to approve
//     let approver_prop_id = get_latest_proposal_id(&app, proposal_single_approver.clone());
//     approve_proposal(
//         &mut app,
//         proposal_single_approver.clone(),
//         "ekez",
//         approver_prop_id,
//     );
//     let native_id = get_latest_proposal_id(&app, proposal_single_approver.clone());

//     // Update the config to use a cw20 token.
//     let cw20_address = instantiate_cw20_base_default(&mut app);
//     update_config(
//         &mut app,
//         pre_propose.clone(),
//         core_addr.as_str(),
//         Some(UncheckedDepositInfo {
//             denom: DepositToken::Token {
//                 denom: UncheckedDenom::Cw20(cw20_address.to_string()),
//             },
//             amount: Uint128::new(10),
//             refund_policy: DepositRefundPolicy::Always,
//         }),
//         false,
//     );

//     increase_allowance(
//         &mut app,
//         "ekez",
//         &pre_propose,
//         cw20_address.clone(),
//         Uint128::new(10),
//     );
//     let _cw20_pre_propose_id = make_pre_proposal(&mut app, pre_propose.clone(), "ekez", &[]);

//     // Approver DAO votes to approve
//     let approver_prop_id = get_latest_proposal_id(&app, proposal_single_approver.clone());
//     approve_proposal(&mut app, proposal_single_approver, "ekez", approver_prop_id);
//     let cw20_id = get_latest_proposal_id(&app, proposal_single.clone());

//     // There is now a pending proposal and cw20 tokens in the
//     // pre-propose module that should be returned on that proposal's
//     // completion. To make things interesting, we withdraw those
//     // tokens which should cause the status change hook on the
//     // proposal's execution to fail as we don't have sufficent balance
//     // to return the deposit.
//     withdraw(&mut app, pre_propose.clone(), core_addr.as_str(), None);
//     let balance = get_balance_cw20(&app, &cw20_address, core_addr.as_str());
//     assert_eq!(balance, Uint128::new(10));

//     // Proposal should still be executable! We just get removed from
//     // the proposal module's hook receiver list.
//     vote(
//         &mut app,
//         proposal_single.clone(),
//         "ekez",
//         cw20_id,
//         Vote::Yes,
//     );
//     execute_proposal(&mut app, proposal_single.clone(), "ekez", cw20_id);

//     // Make sure the proposal module has fallen back to anyone can
//     // propose becuase of our malfunction.
//     let proposal_creation_policy: ProposalCreationPolicy = app
//         .wrap()
//         .query_wasm_smart(
//             proposal_single.clone(),
//             &dps::msg::QueryMsg::ProposalCreationPolicy {},
//         )
//         .unwrap();

//     assert_eq!(proposal_creation_policy, ProposalCreationPolicy::Anyone {});

//     // Close out the native proposal and it's deposit as well.
//     vote(
//         &mut app,
//         proposal_single.clone(),
//         "ekez",
//         native_id,
//         Vote::No,
//     );
//     close_proposal(&mut app, proposal_single.clone(), "ekez", native_id);
//     withdraw(
//         &mut app,
//         pre_propose.clone(),
//         core_addr.as_str(),
//         Some(UncheckedDenom::Native("ujuno".to_string())),
//     );
//     let balance = get_balance_native(&app, core_addr.as_str(), "ujuno");
//     assert_eq!(balance, Uint128::new(30));
// }

// #[test]
// fn test_reset_approver() {
//     let mut app = App::default();

//     // Need to instantiate this so contract addresses match with cw20 test cases
//     let _ = instantiate_cw20_base_default(&mut app);

//     let DefaultTestSetup {
//         core_addr: _,
//         proposal_single: _,
//         pre_propose,
//         approver_core_addr,
//         proposal_single_approver: _,
//         pre_propose_approver,
//     } = setup_default_test(
//         &mut app,
//         Some(UncheckedDepositInfo {
//             denom: DepositToken::Token {
//                 denom: UncheckedDenom::Native("ujuno".to_string()),
//             },
//             amount: Uint128::new(10),
//             refund_policy: DepositRefundPolicy::Always,
//         }),
//         false,
//     );

//     // Ensure approver is set to the pre_propose_approver
//     let approver: Addr = app
//         .wrap()
//         .query_wasm_smart(
//             pre_propose.clone(),
//             &QueryMsg::QueryExtension {
//                 msg: QueryExt::Approver {},
//             },
//         )
//         .unwrap();
//     assert_eq!(approver, pre_propose_approver);

//     // Fail to change approver by non-approver.
//     let err: PreProposeError = app
//         .execute_contract(
//             Addr::unchecked("someone"),
//             pre_propose.clone(),
//             &ExecuteMsg::Extension {
//                 msg: ExecuteExt::UpdateApprover {
//                     address: "someone".to_string(),
//                 },
//             },
//             &[],
//         )
//         .unwrap_err()
//         .downcast()
//         .unwrap();
//     assert_eq!(err, PreProposeError::Unauthorized {});

//     // Fail to reset approver back to approver DAO by non-approver.
//     let err: PreProposeError = app
//         .execute_contract(
//             Addr::unchecked("someone"),
//             pre_propose_approver.clone(),
//             &ApproverExecuteMsg::Extension {
//                 msg: ApproverExecuteExt::ResetApprover {},
//             },
//             &[],
//         )
//         .unwrap_err()
//         .downcast()
//         .unwrap();
//     assert_eq!(err, PreProposeError::Unauthorized {});

//     // Reset approver back to approver DAO.
//     app.execute_contract(
//         approver_core_addr.clone(),
//         pre_propose_approver.clone(),
//         &ApproverExecuteMsg::Extension {
//             msg: ApproverExecuteExt::ResetApprover {},
//         },
//         &[],
//     )
//     .unwrap();

//     // Ensure approver is reset back to the approver DAO
//     let approver: Addr = app
//         .wrap()
//         .query_wasm_smart(
//             pre_propose.clone(),
//             &QueryMsg::QueryExtension {
//                 msg: QueryExt::Approver {},
//             },
//         )
//         .unwrap();
//     assert_eq!(approver, approver_core_addr);
// }
