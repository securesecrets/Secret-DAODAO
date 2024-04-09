use cosmwasm_std::{
    from_binary,
    testing::{mock_dependencies, mock_env, mock_info},
    to_binary, Addr, ContractInfo, Decimal, Empty, MessageInfo, Uint128,
};
use dao_interface::{
    state::AnyContractInfo,
    voting::{InfoResponse, IsActiveResponse, VotingPowerAtHeightResponse},
};
use dao_voting::threshold::{ActiveThreshold, ActiveThresholdResponse};
use schemars::JsonSchema;
use secret_cw2::ContractVersion;
use secret_multi_test::{
    next_block, App, Contract, ContractInstantiationInfo, ContractWrapper, Executor,
};
use secret_utils::Duration;
use serde::{Deserialize, Serialize};
use shade_protocol::{basic_staking::Auth, utils::asset::RawContract};
use snip20_reference_impl::msg::InitialBalance as Snip20InitialBalance;

use crate::{
    contract::{migrate, CONTRACT_NAME, CONTRACT_VERSION},
    msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, StakingInfo},
    snip20_msg::InitialBalance,
};

const DAO_ADDR: &str = "dao";
const CREATOR_ADDR: &str = "creator";

#[derive(Serialize, Deserialize, JsonSchema, Clone, Default, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InitConfig {
    /// Indicates whether the total supply is public or should be kept secret.
    /// default: False
    pub public_total_supply: Option<bool>,
    /// Indicates whether deposit functionality should be enabled
    /// default: False
    pub enable_deposit: Option<bool>,
    /// Indicates whether redeem functionality should be enabled
    /// default: False
    pub enable_redeem: Option<bool>,
    /// Indicates whether mint functionality should be enabled
    /// default: False
    pub enable_mint: Option<bool>,
    /// Indicates whether burn functionality should be enabled
    /// default: False
    pub enable_burn: Option<bool>,
    /// Indicated whether an admin can modify supported denoms
    /// default: False
    pub can_modify_denoms: Option<bool>,
}

fn snip20_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip20_reference_impl::contract::execute,
        snip20_reference_impl::contract::instantiate,
        snip20_reference_impl::contract::query,
    );
    Box::new(contract)
}

fn contract_query_auth() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        query_auth::contract::execute,
        query_auth::contract::instantiate,
        query_auth::contract::query,
    );
    Box::new(contract)
}

fn staking_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip20_stake::contract::execute,
        snip20_stake::contract::instantiate,
        snip20_stake::contract::query,
    );
    Box::new(contract)
}

fn staked_balance_voting_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_reply(crate::contract::reply)
    .with_migrate(crate::contract::migrate);
    Box::new(contract)
}

fn instantiate_voting(
    app: &mut App,
    voting_instantiate_info: ContractInstantiationInfo,
    msg: InstantiateMsg,
) -> ContractInfo {
    app.instantiate_contract(
        voting_instantiate_info,
        Addr::unchecked(DAO_ADDR),
        &msg,
        &[],
        "voting module",
        None,
    )
    .unwrap()
}

fn instantiate_snip20(app: &mut App, initial_balances: Vec<Snip20InitialBalance>) -> ContractInfo {
    let snip20_info = app.store_code(snip20_contract());
    let msg = snip20_reference_impl::msg::InstantiateMsg {
        name: String::from("Test"),
        symbol: String::from("TEST"),
        decimals: 6,
        initial_balances: Some(initial_balances),
        admin: None,
        prng_seed: to_binary("seed").unwrap(),
        config: Some(snip20_reference_impl::msg::InitConfig {
            public_total_supply: Some(true),
            enable_deposit: None,
            enable_redeem: None,
            enable_mint: None,
            enable_burn: None,
            can_modify_denoms: None,
        }),
        supported_denoms: None,
    };

    app.instantiate_contract(
        snip20_info,
        Addr::unchecked(CREATOR_ADDR),
        &msg,
        &[],
        "snip20",
        None,
    )
    .unwrap()
}

fn instantiate_staking(
    app: &mut App,
    snip20: Addr,
    snip20_code_hash: String,
    unstaking_duration: Option<Duration>,
    query_auth: shade_protocol::Contract,
) -> ContractInfo {
    let staking_info = app.store_code(staking_contract());
    let msg = snip20_stake::msg::InstantiateMsg {
        owner: Some(CREATOR_ADDR.to_string()),
        token_address: snip20.to_string(),
        unstaking_duration,
        token_code_hash: Some(snip20_code_hash),
        query_auth: query_auth.into(),
    };
    app.instantiate_contract(
        staking_info,
        Addr::unchecked(CREATOR_ADDR),
        &msg,
        &[],
        "staking",
        Some("admin".to_string()),
    )
    .unwrap()
}

fn instantiate_query_auth(app: &mut App) -> ContractInfo {
    let query_auth_info = app.store_code(contract_query_auth());
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

fn stake_tokens(
    app: &mut App,
    staking_addr: Addr,
    staking_code_hash: String,
    snip20_contract_info: ContractInfo,
    sender: &str,
    amount: u128,
) {
    let msg = snip20_reference_impl::msg::ExecuteMsg::Send {
        recipient: staking_addr.to_string(),
        recipient_code_hash: Some(staking_code_hash),
        amount: Uint128::new(amount),
        msg: Some(to_binary(&snip20_stake::msg::ReceiveMsg::Stake {}).unwrap()),
        memo: None,
        decoys: None,
        entropy: None,
        padding: None,
    };
    app.execute_contract(Addr::unchecked(sender), &snip20_contract_info, &msg, &[])
        .unwrap();
}

fn create_viewing_key(app: &mut App, contract_info: ContractInfo, info: MessageInfo) -> String {
    let msg = shade_protocol::contract_interfaces::query_auth::ExecuteMsg::CreateViewingKey {
        entropy: "entropy".to_string(),
        padding: None,
    };
    let res = app
        .execute_contract(info.sender, &contract_info, &msg, &[])
        .unwrap();
    let mut viewing_key = String::new();
    let data: shade_protocol::contract_interfaces::query_auth::ExecuteAnswer =
        from_binary(&res.data.unwrap()).unwrap();
    match data {
        shade_protocol::contract_interfaces::query_auth::ExecuteAnswer::CreateViewingKey {
            key,
        } => {
            viewing_key = key;
        }
        _ => (),
    };
    viewing_key
}

fn create_snip20_viewing_key(
    app: &mut App,
    contract_info: ContractInfo,
    info: MessageInfo,
) -> String {
    let msg = snip20_reference_impl::msg::ExecuteMsg::CreateViewingKey {
        entropy: "entropy".to_string(),
        padding: None,
    };
    let res = app
        .execute_contract(info.sender, &contract_info, &msg, &[])
        .unwrap();
    let mut viewing_key = String::new();
    let data: snip20_reference_impl::msg::ExecuteAnswer = from_binary(&res.data.unwrap()).unwrap();
    match data {
        snip20_reference_impl::msg::ExecuteAnswer::CreateViewingKey { key } => {
            viewing_key = key;
        }
        _ => (),
    };
    viewing_key
}

#[test]
#[should_panic(expected = "Initial governance token balances must not be empty")]
fn test_instantiate_zero_supply() {
    let mut app = App::default();
    let snip20_instantiate_info = app.store_code(snip20_contract());
    let voting_instantiate_info = app.store_code(staked_balance_voting_contract());
    let staking_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);

    instantiate_voting(
        &mut app,
        voting_instantiate_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::New {
                code_id: snip20_instantiate_info.code_id,
                code_hash: snip20_instantiate_info.code_hash,
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::zero(),
                }],
                unstaking_duration: None,
                staking_code_id: staking_instantiate_info.code_id,
                staking_code_hash: staking_instantiate_info.code_hash,
                initial_dao_balance: Some(Uint128::zero()),
            },
            active_threshold: None,
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.address.to_string(),
                code_hash: query_auth.code_hash,
            },
        },
    );
}

#[test]
#[should_panic(expected = "Initial governance token balances must not be empty")]
fn test_instantiate_no_balances() {
    let mut app = App::default();
    let snip20_instantiate_info = app.store_code(snip20_contract());
    let voting_instantiate_info = app.store_code(staked_balance_voting_contract());
    let staking_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);

    instantiate_voting(
        &mut app,
        voting_instantiate_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::New {
                code_id: snip20_instantiate_info.code_id,
                code_hash: snip20_instantiate_info.code_hash,
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![],
                unstaking_duration: None,
                staking_code_id: staking_instantiate_info.code_id,
                staking_code_hash: staking_instantiate_info.code_hash,
                initial_dao_balance: Some(Uint128::zero()),
            },
            active_threshold: None,
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.address.to_string(),
                code_hash: query_auth.code_hash,
            },
        },
    );
}

#[test]
#[should_panic(expected = "Active threshold count must be greater than zero")]
fn test_instantiate_zero_active_threshold_count() {
    let mut app = App::default();
    let voting_instantiate_info = app.store_code(staked_balance_voting_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let initial_balances = vec![Snip20InitialBalance {
        address: CREATOR_ADDR.to_string(),
        amount: Uint128::from(100u128),
    }];
    let snip20_info = instantiate_snip20(&mut app, initial_balances);
    let staking_info = instantiate_staking(
        &mut app,
        snip20_info.clone().address,
        snip20_info.clone().code_hash,
        None,
        shade_protocol::Contract {
            address: query_auth.clone().address,
            code_hash: query_auth.clone().code_hash,
        },
    );

    instantiate_voting(
        &mut app,
        voting_instantiate_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20_info.clone().address.to_string(),
                code_hash: snip20_info.clone().code_hash,
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_info.clone().address.to_string(),
                    staking_contract_code_hash: staking_info.clone().code_hash,
                },
            },
            active_threshold: Some(ActiveThreshold::AbsoluteCount {
                count: Uint128::new(0),
            }),
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.address.to_string(),
                code_hash: query_auth.code_hash,
            },
        },
    );
}

#[test]
fn test_contract_info() {
    let mut app = App::default();
    let voting_instantiate_info = app.store_code(staked_balance_voting_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let initial_balances = vec![Snip20InitialBalance {
        address: CREATOR_ADDR.to_string(),
        amount: Uint128::from(100u128),
    }];
    let snip20_info = instantiate_snip20(&mut app, initial_balances);
    let staking_info = instantiate_staking(
        &mut app,
        snip20_info.clone().address,
        snip20_info.clone().code_hash,
        None,
        shade_protocol::Contract {
            address: query_auth.clone().address,
            code_hash: query_auth.clone().code_hash,
        },
    );

    let voting_info = instantiate_voting(
        &mut app,
        voting_instantiate_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20_info.clone().address.to_string(),
                code_hash: snip20_info.clone().code_hash,
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_info.clone().address.to_string(),
                    staking_contract_code_hash: staking_info.clone().code_hash,
                },
            },
            active_threshold: None,
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.address.to_string(),
                code_hash: query_auth.code_hash,
            },
        },
    );

    let info: InfoResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::Info {},
        )
        .unwrap();
    assert_eq!(
        info,
        InfoResponse {
            info: ContractVersion {
                contract: "crates.io:dao-voting-snip20-staked".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string()
            }
        }
    );

    let dao: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::Dao {},
        )
        .unwrap();
    assert_eq!(
        dao,
        AnyContractInfo {
            addr: Addr::unchecked(DAO_ADDR),
            code_hash: "dao_code_hash".to_string(),
        }
    );
}

#[test]
fn test_existing_snip20() {
    let mut app = App::default();
    let voting_instantiate_info = app.store_code(staked_balance_voting_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let initial_balances = vec![
        Snip20InitialBalance {
            address: DAO_ADDR.to_string(),
            amount: Uint128::from(100u128),
        },
        Snip20InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::from(100u128),
        },
    ];
    let snip20_info = instantiate_snip20(&mut app, initial_balances);
    let staking_contract_info = instantiate_staking(
        &mut app,
        snip20_info.clone().address,
        snip20_info.clone().code_hash,
        None,
        shade_protocol::Contract {
            address: query_auth.clone().address,
            code_hash: query_auth.clone().code_hash,
        },
    );

    let voting_info = instantiate_voting(
        &mut app,
        voting_instantiate_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20_info.clone().address.to_string(),
                code_hash: snip20_info.clone().code_hash,
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_contract_info.clone().address.to_string(),
                    staking_contract_code_hash: staking_contract_info.clone().code_hash,
                },
            },
            active_threshold: None,
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            },
        },
    );

    let dao_addr_snip20_viewing_key =
        create_snip20_viewing_key(&mut app, snip20_info.clone(), mock_info(DAO_ADDR, &[]));
    // Expect DAO (sender address) to have initial balance.
    let token_info: snip20_reference_impl::msg::QueryAnswer = app
        .wrap()
        .query_wasm_smart(
            snip20_info.clone().code_hash,
            snip20_info.clone().address.to_string(),
            &snip20_reference_impl::msg::QueryMsg::Balance {
                address: DAO_ADDR.to_string(),
                key: dao_addr_snip20_viewing_key.clone(),
            },
        )
        .unwrap();
    let mut balance = Uint128::zero();
    match token_info {
        snip20_reference_impl::msg::QueryAnswer::Balance { amount } => {
            balance = amount;
        }
        _ => (),
    }
    assert_eq!(balance, Uint128::from(100u64));

    let creator_viewing_key_snip20_stake =
        create_viewing_key(&mut app, query_auth.clone(), mock_info(CREATOR_ADDR, &[]));
    // Expect 0 as they have not staked
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: creator_viewing_key_snip20_stake.clone(),
                    address: CREATOR_ADDR.to_string(),
                },
                height: None,
            },
        )
        .unwrap();

    assert_eq!(
        creator_voting_power,
        VotingPowerAtHeightResponse {
            power: Uint128::zero(),
            height: app.block_info().height,
        }
    );

    // Stake 1 token as creator
    stake_tokens(
        &mut app,
        staking_contract_info.clone().address,
        staking_contract_info.clone().code_hash,
        snip20_info,
        CREATOR_ADDR,
        1,
    );
    app.update_block(next_block);

    // Expect 1 as creator has now staked 1
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: creator_viewing_key_snip20_stake.clone(),
                    address: CREATOR_ADDR.to_string(),
                },
                height: None,
            },
        )
        .unwrap();

    assert_eq!(
        creator_voting_power,
        VotingPowerAtHeightResponse {
            power: Uint128::new(1u128),
            height: app.block_info().height,
        }
    );

    // Expect 1 as only one token staked to make up whole voting power
    let total_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::TotalPowerAtHeight { height: None },
        )
        .unwrap();

    assert_eq!(
        total_voting_power,
        VotingPowerAtHeightResponse {
            power: Uint128::new(1u128),
            height: app.block_info().height,
        }
    )
}

#[test]
fn test_existing_cw20_existing_staking() {
    let mut app = App::default();
    let voting_instantiate_info = app.store_code(staked_balance_voting_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let initial_balances = vec![
        Snip20InitialBalance {
            address: DAO_ADDR.to_string(),
            amount: Uint128::from(100u128),
        },
        Snip20InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::from(2u128),
        },
    ];
    let snip20_info = instantiate_snip20(&mut app, initial_balances);
    // We'll use this for our valid existing contract
    let staking_contract_info = instantiate_staking(
        &mut app,
        snip20_info.clone().address,
        snip20_info.clone().code_hash,
        None,
        shade_protocol::Contract {
            address: query_auth.clone().address,
            code_hash: query_auth.clone().code_hash,
        },
    );

    let voting_info = instantiate_voting(
        &mut app,
        voting_instantiate_info.clone(),
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20_info.clone().address.to_string(),
                code_hash: snip20_info.clone().code_hash,
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_contract_info.clone().address.to_string(),
                    staking_contract_code_hash: staking_contract_info.clone().code_hash,
                },
            },
            active_threshold: None,
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            },
        },
    );

    // Expect 0 as creator has not staked
    let creator_viewing_key_snip20_stake =
        create_viewing_key(&mut app, query_auth.clone(), mock_info(CREATOR_ADDR, &[]));
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: creator_viewing_key_snip20_stake.clone(),
                    address: CREATOR_ADDR.to_string(),
                },
                height: None,
            },
        )
        .unwrap();

    assert_eq!(
        creator_voting_power,
        VotingPowerAtHeightResponse {
            power: Uint128::zero(),
            height: app.block_info().height,
        }
    );

    // Expect 0 as DAO has not staked
    let dao_viewing_key_snip20_stake =
        create_viewing_key(&mut app, query_auth.clone(), mock_info(DAO_ADDR, &[]));
    let dao_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: dao_viewing_key_snip20_stake.clone(),
                    address: DAO_ADDR.to_string(),
                },
                height: None,
            },
        )
        .unwrap();

    assert_eq!(
        dao_voting_power,
        VotingPowerAtHeightResponse {
            power: Uint128::zero(),
            height: app.block_info().height,
        }
    );

    // Stake 1 token as creator
    stake_tokens(
        &mut app,
        staking_contract_info.clone().address,
        staking_contract_info.clone().code_hash,
        snip20_info.clone(),
        CREATOR_ADDR,
        1,
    );

    // Expect 1 as creator has now staked 1
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: creator_viewing_key_snip20_stake.clone(),
                    address: CREATOR_ADDR.to_string(),
                },
                height: None,
            },
        )
        .unwrap();

    assert_eq!(
        creator_voting_power,
        VotingPowerAtHeightResponse {
            power: Uint128::new(1u128),
            height: app.block_info().height,
        }
    );

    // Expect 1 as only one token staked to make up whole voting power
    let total_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::TotalPowerAtHeight { height: None },
        )
        .unwrap();

    assert_eq!(
        total_voting_power,
        VotingPowerAtHeightResponse {
            power: Uint128::new(1u128),
            height: app.block_info().height,
        }
    );

    // Now lets test the error case where we use an invalid staking contract
    // Expect error as the token address does not match the staking address token address
    app.instantiate_contract(
        voting_instantiate_info,
        Addr::unchecked(DAO_ADDR),
        &InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: "different_token".to_string(),
                code_hash: "different_token_code_hash".to_string(),
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_contract_info.clone().address.to_string(),
                    staking_contract_code_hash: staking_contract_info.clone().code_hash,
                },
            },
            active_threshold: None,
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.address.to_string(),
                code_hash: query_auth.code_hash,
            },
        },
        &[],
        "voting module",
        None,
    )
    .unwrap_err();
}

#[test]
fn test_different_heights() {
    let mut app = App::default();
    let voting_instantiate_info = app.store_code(staked_balance_voting_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let initial_balances = vec![
        Snip20InitialBalance {
            address: DAO_ADDR.to_string(),
            amount: Uint128::from(100u128),
        },
        Snip20InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::from(2u128),
        },
    ];
    let snip20_info = instantiate_snip20(&mut app, initial_balances);
    // We'll use this for our valid existing contract
    let staking_contract_info = instantiate_staking(
        &mut app,
        snip20_info.clone().address,
        snip20_info.clone().code_hash,
        None,
        shade_protocol::Contract {
            address: query_auth.clone().address,
            code_hash: query_auth.clone().code_hash,
        },
    );

    let voting_info = instantiate_voting(
        &mut app,
        voting_instantiate_info.clone(),
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20_info.clone().address.to_string(),
                code_hash: snip20_info.clone().code_hash,
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_contract_info.clone().address.to_string(),
                    staking_contract_code_hash: staking_contract_info.clone().code_hash,
                },
            },
            active_threshold: None,
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            },
        },
    );

    // Expect 0 as creator has not staked
    let creator_viewing_key_snip20_stake =
        create_viewing_key(&mut app, query_auth.clone(), mock_info(CREATOR_ADDR, &[]));
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: creator_viewing_key_snip20_stake.clone(),
                    address: CREATOR_ADDR.to_string(),
                },
                height: None,
            },
        )
        .unwrap();

    assert_eq!(
        creator_voting_power,
        VotingPowerAtHeightResponse {
            power: Uint128::zero(),
            height: app.block_info().height,
        }
    );
    // Stake 1 token as creator
    stake_tokens(
        &mut app,
        staking_contract_info.clone().address,
        staking_contract_info.clone().code_hash,
        snip20_info.clone(),
        CREATOR_ADDR,
        1,
    );

    // Expect 1 as creator has now staked 1
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: creator_viewing_key_snip20_stake.clone(),
                    address: CREATOR_ADDR.to_string(),
                },
                height: None,
            },
        )
        .unwrap();

    assert_eq!(
        creator_voting_power,
        VotingPowerAtHeightResponse {
            power: Uint128::new(1u128),
            height: app.block_info().height,
        }
    );

    // Expect 1 as only one token staked to make up whole voting power
    let total_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::TotalPowerAtHeight { height: None },
        )
        .unwrap();

    assert_eq!(
        total_voting_power,
        VotingPowerAtHeightResponse {
            power: Uint128::new(1u128),
            height: app.block_info().height,
        }
    );

    app.update_block(next_block);
    // Stake another 1 token as creator
    stake_tokens(
        &mut app,
        staking_contract_info.clone().address,
        staking_contract_info.clone().code_hash,
        snip20_info.clone(),
        CREATOR_ADDR,
        1,
    );

    // Expect 2 as creator has now staked 2
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: creator_viewing_key_snip20_stake.clone(),
                    address: CREATOR_ADDR.to_string(),
                },
                height: None,
            },
        )
        .unwrap();

    assert_eq!(
        creator_voting_power,
        VotingPowerAtHeightResponse {
            power: Uint128::new(2u128),
            height: app.block_info().height,
        }
    );

    // Expect 2 as 2 token staked to make up whole voting power
    let total_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::TotalPowerAtHeight { height: None },
        )
        .unwrap();

    assert_eq!(
        total_voting_power,
        VotingPowerAtHeightResponse {
            power: Uint128::new(2u128),
            height: app.block_info().height,
        }
    );

    // Check we can query history
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: creator_viewing_key_snip20_stake.clone(),
                    address: CREATOR_ADDR.to_string(),
                },
                height: Some(app.block_info().height - 1),
            },
        )
        .unwrap();

    assert_eq!(
        creator_voting_power,
        VotingPowerAtHeightResponse {
            power: Uint128::new(1u128),
            height: app.block_info().height - 1,
        }
    );

    // Expect 1 at the old height prior to second stake
    let total_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::TotalPowerAtHeight {
                height: Some(app.block_info().height - 1),
            },
        )
        .unwrap();

    assert_eq!(
        total_voting_power,
        VotingPowerAtHeightResponse {
            power: Uint128::new(1u128),
            height: app.block_info().height - 1,
        }
    );
}

#[test]
fn test_active_threshold_absolute_count() {
    let mut app = App::default();
    let voting_instantiate_info = app.store_code(staked_balance_voting_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let initial_balances = vec![
        Snip20InitialBalance {
            address: DAO_ADDR.to_string(),
            amount: Uint128::from(100u128),
        },
        Snip20InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::from(200u128),
        },
    ];
    let snip20_info = instantiate_snip20(&mut app, initial_balances);

    let token_info: snip20_reference_impl::msg::QueryAnswer = app
        .wrap()
        .query_wasm_smart(
            snip20_info.clone().code_hash,
            snip20_info.clone().address.to_string(),
            &secret_toolkit::snip20::QueryMsg::TokenInfo {},
        )
        .unwrap();
    println!("{:?}", token_info);
    // We'll use this for our valid existing contract
    let staking_contract_info = instantiate_staking(
        &mut app,
        snip20_info.clone().address,
        snip20_info.clone().code_hash,
        None,
        shade_protocol::Contract {
            address: query_auth.clone().address,
            code_hash: query_auth.clone().code_hash,
        },
    );

    let voting_info = instantiate_voting(
        &mut app,
        voting_instantiate_info.clone(),
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20_info.clone().address.to_string(),
                code_hash: snip20_info.clone().code_hash,
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_contract_info.clone().address.to_string(),
                    staking_contract_code_hash: staking_contract_info.clone().code_hash,
                },
            },
            active_threshold: Some(ActiveThreshold::AbsoluteCount {
                count: Uint128::new(100),
            }),
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.address.to_string(),
                code_hash: query_auth.code_hash,
            },
        },
    );

    // Not active as none staked
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(!is_active.active);

    // Stake 100 token as creator
    app.update_block(next_block);
    stake_tokens(
        &mut app,
        staking_contract_info.clone().address,
        staking_contract_info.clone().code_hash,
        snip20_info.clone(),
        CREATOR_ADDR,
        100,
    );

    // Active as enough staked
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(is_active.active);
}

#[test]
fn test_active_threshold_percent() {
    let mut app = App::default();
    let voting_instantiate_info = app.store_code(staked_balance_voting_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let initial_balances = vec![
        Snip20InitialBalance {
            address: DAO_ADDR.to_string(),
            amount: Uint128::from(100u128),
        },
        Snip20InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::from(200u128),
        },
    ];
    let snip20_info = instantiate_snip20(&mut app, initial_balances);

    let token_info: snip20_reference_impl::msg::QueryAnswer = app
        .wrap()
        .query_wasm_smart(
            snip20_info.clone().code_hash,
            snip20_info.clone().address.to_string(),
            &secret_toolkit::snip20::QueryMsg::TokenInfo {},
        )
        .unwrap();
    println!("{:?}", token_info);
    // We'll use this for our valid existing contract
    let staking_contract_info = instantiate_staking(
        &mut app,
        snip20_info.clone().address,
        snip20_info.clone().code_hash,
        None,
        shade_protocol::Contract {
            address: query_auth.clone().address,
            code_hash: query_auth.clone().code_hash,
        },
    );

    let voting_info = instantiate_voting(
        &mut app,
        voting_instantiate_info.clone(),
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20_info.clone().address.to_string(),
                code_hash: snip20_info.clone().code_hash,
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_contract_info.clone().address.to_string(),
                    staking_contract_code_hash: staking_contract_info.clone().code_hash,
                },
            },
            active_threshold: Some(ActiveThreshold::Percentage {
                percent: Decimal::percent(20),
            }),
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.address.to_string(),
                code_hash: query_auth.code_hash,
            },
        },
    );

    // Not active as none staked
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(!is_active.active);

    // Stake 60 token as creator, now active
    app.update_block(next_block);
    stake_tokens(
        &mut app,
        staking_contract_info.clone().address,
        staking_contract_info.clone().code_hash,
        snip20_info.clone(),
        CREATOR_ADDR,
        60,
    );
    // Active as enough staked
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(is_active.active);
}

#[test]
fn test_active_threshold_percent_rounds_up() {
    let mut app = App::default();
    let voting_instantiate_info = app.store_code(staked_balance_voting_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let initial_balances = vec![
        Snip20InitialBalance {
            address: DAO_ADDR.to_string(),
            amount: Uint128::from(100u128),
        },
        Snip20InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::from(200u128),
        },
    ];
    let snip20_info = instantiate_snip20(&mut app, initial_balances);

    let token_info: snip20_reference_impl::msg::QueryAnswer = app
        .wrap()
        .query_wasm_smart(
            snip20_info.clone().code_hash,
            snip20_info.clone().address.to_string(),
            &secret_toolkit::snip20::QueryMsg::TokenInfo {},
        )
        .unwrap();
    println!("{:?}", token_info);
    // We'll use this for our valid existing contract
    let staking_contract_info = instantiate_staking(
        &mut app,
        snip20_info.clone().address,
        snip20_info.clone().code_hash,
        None,
        shade_protocol::Contract {
            address: query_auth.clone().address,
            code_hash: query_auth.clone().code_hash,
        },
    );

    let voting_info = instantiate_voting(
        &mut app,
        voting_instantiate_info.clone(),
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20_info.clone().address.to_string(),
                code_hash: snip20_info.clone().code_hash,
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_contract_info.clone().address.to_string(),
                    staking_contract_code_hash: staking_contract_info.clone().code_hash,
                },
            },
            active_threshold: Some(ActiveThreshold::Percentage {
                percent: Decimal::percent(50),
            }),
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.address.to_string(),
                code_hash: query_auth.code_hash,
            },
        },
    );

    // Not active as none staked
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(!is_active.active);

    // Stake 2 token as creator, should not be active
    app.update_block(next_block);
    stake_tokens(
        &mut app,
        staking_contract_info.clone().address,
        staking_contract_info.clone().code_hash,
        snip20_info.clone(),
        CREATOR_ADDR,
        2,
    );

    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(!is_active.active);

    // Stake 1 more token as creator, should now be active.
    app.update_block(next_block);
    stake_tokens(
        &mut app,
        staking_contract_info.clone().address,
        staking_contract_info.clone().code_hash,
        snip20_info.clone(),
        CREATOR_ADDR,
        1,
    );

    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(!is_active.active);
}

#[test]
fn test_active_threshold_none() {
    let mut app = App::default();
    let voting_instantiate_info = app.store_code(staked_balance_voting_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let initial_balances = vec![
        Snip20InitialBalance {
            address: DAO_ADDR.to_string(),
            amount: Uint128::from(100u128),
        },
        Snip20InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::from(200u128),
        },
    ];
    let snip20_info = instantiate_snip20(&mut app, initial_balances);

    let token_info: snip20_reference_impl::msg::QueryAnswer = app
        .wrap()
        .query_wasm_smart(
            snip20_info.clone().code_hash,
            snip20_info.clone().address.to_string(),
            &secret_toolkit::snip20::QueryMsg::TokenInfo {},
        )
        .unwrap();
    println!("{:?}", token_info);
    // We'll use this for our valid existing contract
    let staking_contract_info = instantiate_staking(
        &mut app,
        snip20_info.clone().address,
        snip20_info.clone().code_hash,
        None,
        shade_protocol::Contract {
            address: query_auth.clone().address,
            code_hash: query_auth.clone().code_hash,
        },
    );

    let voting_info = instantiate_voting(
        &mut app,
        voting_instantiate_info.clone(),
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20_info.clone().address.to_string(),
                code_hash: snip20_info.clone().code_hash,
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_contract_info.clone().address.to_string(),
                    staking_contract_code_hash: staking_contract_info.clone().code_hash,
                },
            },
            active_threshold: None,
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.address.to_string(),
                code_hash: query_auth.code_hash,
            },
        },
    );

    // Active as no threshold
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(is_active.active);
}

#[test]
fn test_update_active_threshold() {
    let mut app = App::default();
    let voting_instantiate_info = app.store_code(staked_balance_voting_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let initial_balances = vec![
        Snip20InitialBalance {
            address: DAO_ADDR.to_string(),
            amount: Uint128::from(100u128),
        },
        Snip20InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::from(200u128),
        },
    ];
    let snip20_info = instantiate_snip20(&mut app, initial_balances);

    let token_info: snip20_reference_impl::msg::QueryAnswer = app
        .wrap()
        .query_wasm_smart(
            snip20_info.clone().code_hash,
            snip20_info.clone().address.to_string(),
            &secret_toolkit::snip20::QueryMsg::TokenInfo {},
        )
        .unwrap();
    println!("{:?}", token_info);
    // We'll use this for our valid existing contract
    let staking_contract_info = instantiate_staking(
        &mut app,
        snip20_info.clone().address,
        snip20_info.clone().code_hash,
        None,
        shade_protocol::Contract {
            address: query_auth.clone().address,
            code_hash: query_auth.clone().code_hash,
        },
    );

    let voting_info = instantiate_voting(
        &mut app,
        voting_instantiate_info.clone(),
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20_info.clone().address.to_string(),
                code_hash: snip20_info.clone().code_hash,
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_contract_info.clone().address.to_string(),
                    staking_contract_code_hash: staking_contract_info.clone().code_hash,
                },
            },
            active_threshold: None,
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.address.to_string(),
                code_hash: query_auth.code_hash,
            },
        },
    );

    let resp: ActiveThresholdResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.clone().code_hash,
            voting_info.clone().address.to_string(),
            &QueryMsg::ActiveThreshold {},
        )
        .unwrap();
    assert_eq!(resp.active_threshold, None);

    let msg = ExecuteMsg::UpdateActiveThreshold {
        new_threshold: Some(ActiveThreshold::AbsoluteCount {
            count: Uint128::new(100),
        }),
    };

    // Expect failure as sender is not the DAO
    app.execute_contract(
        Addr::unchecked(CREATOR_ADDR),
        &voting_info.clone(),
        &msg,
        &[],
    )
    .unwrap_err();

    // Expect success as sender is the DAO
    app.execute_contract(Addr::unchecked(DAO_ADDR), &voting_info.clone(), &msg, &[])
        .unwrap();

    let resp: ActiveThresholdResponse = app
        .wrap()
        .query_wasm_smart(
            voting_info.code_hash,
            voting_info.address.to_string(),
            &QueryMsg::ActiveThreshold {},
        )
        .unwrap();
    assert_eq!(
        resp.active_threshold,
        Some(ActiveThreshold::AbsoluteCount {
            count: Uint128::new(100)
        })
    );
}

#[test]
#[should_panic(expected = "Active threshold percentage must be greater than 0 and less than 1")]
fn test_active_threshold_percentage_gt_100() {
    let mut app = App::default();
    let voting_instantiate_info = app.store_code(staked_balance_voting_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let initial_balances = vec![
        Snip20InitialBalance {
            address: DAO_ADDR.to_string(),
            amount: Uint128::from(100u128),
        },
        Snip20InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::from(200u128),
        },
    ];
    let snip20_info = instantiate_snip20(&mut app, initial_balances);

    let token_info: snip20_reference_impl::msg::QueryAnswer = app
        .wrap()
        .query_wasm_smart(
            snip20_info.clone().code_hash,
            snip20_info.clone().address.to_string(),
            &secret_toolkit::snip20::QueryMsg::TokenInfo {},
        )
        .unwrap();
    println!("{:?}", token_info);
    // We'll use this for our valid existing contract
    let staking_contract_info = instantiate_staking(
        &mut app,
        snip20_info.clone().address,
        snip20_info.clone().code_hash,
        None,
        shade_protocol::Contract {
            address: query_auth.clone().address,
            code_hash: query_auth.clone().code_hash,
        },
    );

    instantiate_voting(
        &mut app,
        voting_instantiate_info.clone(),
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20_info.clone().address.to_string(),
                code_hash: snip20_info.clone().code_hash,
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_contract_info.clone().address.to_string(),
                    staking_contract_code_hash: staking_contract_info.clone().code_hash,
                },
            },
            active_threshold: Some(ActiveThreshold::Percentage {
                percent: Decimal::percent(120),
            }),
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.address.to_string(),
                code_hash: query_auth.code_hash,
            },
        },
    );
}

#[test]
#[should_panic(expected = "Active threshold percentage must be greater than 0 and less than 1")]
fn test_active_threshold_percentage_lte_0() {
    let mut app = App::default();
    let voting_instantiate_info = app.store_code(staked_balance_voting_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let initial_balances = vec![
        Snip20InitialBalance {
            address: DAO_ADDR.to_string(),
            amount: Uint128::from(100u128),
        },
        Snip20InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::from(200u128),
        },
    ];
    let snip20_info = instantiate_snip20(&mut app, initial_balances);

    let token_info: snip20_reference_impl::msg::QueryAnswer = app
        .wrap()
        .query_wasm_smart(
            snip20_info.clone().code_hash,
            snip20_info.clone().address.to_string(),
            &secret_toolkit::snip20::QueryMsg::TokenInfo {},
        )
        .unwrap();
    println!("{:?}", token_info);
    // We'll use this for our valid existing contract
    let staking_contract_info = instantiate_staking(
        &mut app,
        snip20_info.clone().address,
        snip20_info.clone().code_hash,
        None,
        shade_protocol::Contract {
            address: query_auth.clone().address,
            code_hash: query_auth.clone().code_hash,
        },
    );

    instantiate_voting(
        &mut app,
        voting_instantiate_info.clone(),
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20_info.clone().address.to_string(),
                code_hash: snip20_info.clone().code_hash,
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_contract_info.clone().address.to_string(),
                    staking_contract_code_hash: staking_contract_info.clone().code_hash,
                },
            },
            active_threshold: Some(ActiveThreshold::Percentage {
                percent: Decimal::percent(0),
            }),
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.address.to_string(),
                code_hash: query_auth.code_hash,
            },
        },
    );
}

#[test]
#[should_panic(expected = "Absolute count threshold cannot be greater than the total token supply")]
fn test_active_threshold_absolute_count_invalid() {
    let mut app = App::default();
    let voting_instantiate_info = app.store_code(staked_balance_voting_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let initial_balances = vec![
        Snip20InitialBalance {
            address: DAO_ADDR.to_string(),
            amount: Uint128::from(100u128),
        },
        Snip20InitialBalance {
            address: CREATOR_ADDR.to_string(),
            amount: Uint128::from(200u128),
        },
    ];
    let snip20_info = instantiate_snip20(&mut app, initial_balances);

    let token_info: snip20_reference_impl::msg::QueryAnswer = app
        .wrap()
        .query_wasm_smart(
            snip20_info.clone().code_hash,
            snip20_info.clone().address.to_string(),
            &secret_toolkit::snip20::QueryMsg::TokenInfo {},
        )
        .unwrap();
    println!("{:?}", token_info);
    // We'll use this for our valid existing contract
    let staking_contract_info = instantiate_staking(
        &mut app,
        snip20_info.clone().address,
        snip20_info.clone().code_hash,
        None,
        shade_protocol::Contract {
            address: query_auth.clone().address,
            code_hash: query_auth.clone().code_hash,
        },
    );

    instantiate_voting(
        &mut app,
        voting_instantiate_info.clone(),
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20_info.clone().address.to_string(),
                code_hash: snip20_info.clone().code_hash,
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_contract_info.clone().address.to_string(),
                    staking_contract_code_hash: staking_contract_info.clone().code_hash,
                },
            },
            active_threshold: Some(ActiveThreshold::AbsoluteCount {
                count: Uint128::new(10000),
            }),
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: RawContract {
                address: query_auth.address.to_string(),
                code_hash: query_auth.code_hash,
            },
        },
    );
}

#[test]
pub fn test_migrate_update_version() {
    let mut deps = mock_dependencies();
    secret_cw2::set_contract_version(&mut deps.storage, "my-contract", "1.0.0").unwrap();
    migrate(deps.as_mut(), mock_env(), MigrateMsg {}).unwrap();
    let version = secret_cw2::get_contract_version(&deps.storage).unwrap();
    assert_eq!(version.version, CONTRACT_VERSION);
    assert_eq!(version.contract, CONTRACT_NAME);
}
