use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    from_binary,
    testing::{mock_dependencies, mock_env},
    to_binary, Addr, ContractInfo, Decimal, Empty, Uint128,
};

use dao_interface::{
    msg::InitialBalance,
    state::AnyContractInfo,
    voting::{InfoResponse, IsActiveResponse, VotingPowerAtHeightResponse},
};
use dao_voting::threshold::{ActiveThreshold, ActiveThresholdResponse};
use secret_cw2::ContractVersion;
use secret_multi_test::{
    next_block, App, Contract, ContractInstantiationInfo, ContractWrapper, Executor,
};
use shade_protocol::{basic_staking::Auth, utils::asset::RawContract};
use snip20_reference_impl::msg::{InitConfig, InitialBalance as Snip20InitialBalance, QueryAnswer};

use crate::{
    contract::{migrate, CONTRACT_NAME, CONTRACT_VERSION},
    msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, StakingInfo},
};

const DAO_ADDR: &str = "dao";
const CREATOR_ADDR: &str = "creator";

#[cw_serde]
pub struct TokenInfo {
    name: String,
    symbol: String,
    decimals: u8,
    total_supply: Option<Uint128>,
}

#[cw_serde]
pub struct Minters {
    minters: Vec<Addr>,
}

#[cw_serde]
pub struct Balance {
    amount: Uint128,
}

fn contract_query_auth() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        query_auth::contract::execute,
        query_auth::contract::instantiate,
        query_auth::contract::query,
    );
    Box::new(contract)
}

fn snip20_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        snip20_reference_impl::contract::execute,
        snip20_reference_impl::contract::instantiate,
        snip20_reference_impl::contract::query,
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
    voting_contract_instantiation_info: ContractInstantiationInfo,
    msg: InstantiateMsg,
) -> ContractInfo {
    app.instantiate_contract(
        voting_contract_instantiation_info,
        Addr::unchecked(DAO_ADDR),
        &msg,
        &[],
        "voting module",
        None,
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
    auth: Auth,
    amount: u128,
) {
    let msg = snip20_reference_impl::msg::ExecuteMsg::Send {
        recipient: staking_addr.to_string(),
        recipient_code_hash: Some(staking_code_hash),
        amount: Uint128::new(amount),
        msg: Some(
            to_binary(&snip20_stake::msg::ReceiveMsg::Stake {
                auth: Box::new(auth),
            })
            .unwrap(),
        ),
        memo: None,
        decoys: None,
        entropy: None,
        padding: None,
    };
    app.execute_contract(Addr::unchecked(sender), &snip20_contract_info, &msg, &[])
        .unwrap();
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

fn create_snip20_viewing_key(app: &mut App, contract_info: ContractInfo, sender: &str) -> String {
    let msg = snip20_reference_impl::msg::ExecuteMsg::CreateViewingKey {
        entropy: "entropy".to_string(),
        padding: None,
    };
    let res = app
        .execute_contract(Addr::unchecked(sender), &contract_info, &msg, &[])
        .unwrap();
    let mut viewing_key = String::new();
    let data: snip20_reference_impl::msg::ExecuteAnswer = from_binary(&res.data.unwrap()).unwrap();
    if let snip20_reference_impl::msg::ExecuteAnswer::CreateViewingKey { key } = data {
        viewing_key = key;
    }
    viewing_key
}

#[test]
#[should_panic(expected = "Initial governance token balances must not be empty")]
fn test_instantiate_zero_supply() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_contract_info = app.store_code(staking_contract());
    instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::zero(),
                }],
                unstaking_duration: None,
                staking_code_id: staking_contract_info.code_id,
                staking_code_hash: staking_contract_info.code_hash,
                initial_dao_balance: Some(Uint128::zero()),
            },
            active_threshold: None,
            dao_code_hash: "".into(),
            query_auth: None,
        },
    );
}

#[test]
#[should_panic(expected = "Initial governance token balances must not be empty")]
fn test_instantiate_no_balances() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_contract_info = app.store_code(staking_contract());
    instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![],
                unstaking_duration: None,
                staking_code_id: staking_contract_info.code_id,
                staking_code_hash: staking_contract_info.code_hash,
                initial_dao_balance: Some(Uint128::zero()),
            },
            active_threshold: None,
            dao_code_hash: "".into(),
            query_auth: None,
        },
    );
}

#[test]
#[should_panic(expected = "Active threshold count must be greater than zero")]
fn test_instantiate_zero_active_threshold_count() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_contract_info = app.store_code(staking_contract());
    instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::one(),
                }],
                unstaking_duration: None,
                staking_code_id: staking_contract_info.code_id,
                staking_code_hash: staking_contract_info.code_hash,
                initial_dao_balance: Some(Uint128::zero()),
            },
            active_threshold: Some(ActiveThreshold::AbsoluteCount {
                count: Uint128::new(0),
            }),
            query_auth: None,
            dao_code_hash: "".into(),
        },
    );
}

#[test]
fn test_contract_info() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_contract_info = app.store_code(staking_contract());

    let voting_contract_info = instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::from(2u64),
                }],
                unstaking_duration: None,
                staking_code_id: staking_contract_info.code_id,
                staking_code_hash: staking_contract_info.code_hash,
                initial_dao_balance: Some(Uint128::zero()),
            },
            active_threshold: None,
            query_auth: None,
            dao_code_hash: "".into(),
        },
    );

    let info: InfoResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
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
            voting_contract_info.code_hash,
            voting_contract_info.address,
            &QueryMsg::Dao {},
        )
        .unwrap();
    assert_eq!(
        dao,
        AnyContractInfo {
            addr: Addr::unchecked(DAO_ADDR),
            code_hash: "".into()
        }
    );
}

#[test]
fn test_new_snip20() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_contract_info = app.store_code(staking_contract());
    let query_auth_info = instantiate_query_auth(&mut app);

    let viewing_key_creator = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.address.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        CREATOR_ADDR,
    );
    let viewing_key_dao = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.address.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        DAO_ADDR,
    );

    let voting_contract_info = instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::from(2u64),
                }],
                unstaking_duration: None,
                staking_code_id: staking_contract_info.code_id,
                staking_code_hash: staking_contract_info.code_hash,
                initial_dao_balance: Some(Uint128::from(10u64)),
            },
            active_threshold: None,
            query_auth: Some(RawContract::new(
                &query_auth_info.address.clone().to_string(),
                &query_auth_info.code_hash.clone(),
            )),
            dao_code_hash: "".into(),
        },
    );

    let snip20token_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::TokenContract {},
        )
        .unwrap();
    let staking_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::StakingContract {},
        )
        .unwrap();

    let mut token_info = TokenInfo {
        name: "".into(),
        symbol: "".into(),
        decimals: 0,
        total_supply: None,
    };
    let res: QueryAnswer = app
        .wrap()
        .query_wasm_smart(
            snip20token_info.code_hash.clone(),
            snip20token_info.addr.clone(),
            &snip20_reference_impl::msg::QueryMsg::TokenInfo {},
        )
        .unwrap();
    if let QueryAnswer::TokenInfo {
        name,
        symbol,
        decimals,
        total_supply,
    } = res
    {
        token_info.name = name;
        token_info.symbol = symbol;
        token_info.decimals = decimals;
        token_info.total_supply = total_supply;
    }
    assert_eq!(
        token_info,
        TokenInfo {
            name: "DAO DAO".to_string(),
            symbol: "DAO".to_string(),
            decimals: 6,
            total_supply: Some(Uint128::from(12u64))
        }
    );

    let mut minter = Minters { minters: vec![] };
    let res: QueryAnswer = app
        .wrap()
        .query_wasm_smart(
            snip20token_info.code_hash.clone(),
            snip20token_info.addr.clone(),
            &snip20_reference_impl::msg::QueryMsg::Minters {},
        )
        .unwrap();
    if let QueryAnswer::Minters { minters } = res {
        minter.minters = minters;
    }

    assert_eq!(
        minter,
        Minters {
            minters: vec![Addr::unchecked(DAO_ADDR)],
        }
    );

    let token_viewing_key = create_snip20_viewing_key(
        &mut app,
        ContractInfo {
            address: snip20token_info.addr.clone(),
            code_hash: snip20token_info.code_hash.clone(),
        },
        DAO_ADDR,
    );

    let mut balance = Balance {
        amount: Uint128::zero(),
    };
    // Expect DAO (sender address) to have initial balance.
    let res: QueryAnswer = app
        .wrap()
        .query_wasm_smart(
            snip20token_info.code_hash.clone(),
            snip20token_info.addr.clone(),
            &snip20_reference_impl::msg::QueryMsg::Balance {
                address: DAO_ADDR.to_string(),
                key: token_viewing_key.clone(),
            },
        )
        .unwrap();

    if let QueryAnswer::Balance { amount } = res {
        balance.amount = amount;
    }
    assert_eq!(
        balance,
        Balance {
            amount: Uint128::from(10u64)
        }
    );

    // Expect 0 as they have not staked
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key_creator.clone(),
                    address: CREATOR_ADDR.into(),
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
    let dao_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key_dao.clone(),
                    address: DAO_ADDR.into(),
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
        staking_info.addr,
        staking_info.code_hash,
        ContractInfo {
            address: snip20token_info.addr.clone(),
            code_hash: snip20token_info.code_hash.clone(),
        },
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key_creator.clone(),
            address: CREATOR_ADDR.into(),
        },
        1,
    );
    app.update_block(next_block);

    // Expect 1 as creator has now staked 1
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key_creator.clone(),
                    address: CREATOR_ADDR.into(),
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
            voting_contract_info.code_hash,
            voting_contract_info.address,
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
fn test_existing_snip20_new_staking() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_info = app.store_code(staking_contract());

    let query_auth_info = instantiate_query_auth(&mut app);

    let viewing_key_creator = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.address.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        CREATOR_ADDR,
    );
    let viewing_key_dao = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.address.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        DAO_ADDR,
    );

    let snip20token_info = app
        .instantiate_contract(
            snip20_info,
            Addr::unchecked(CREATOR_ADDR),
            &snip20_reference_impl::msg::InstantiateMsg {
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 3,
                initial_balances: Some(vec![Snip20InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::from(2u64),
                }]),
                admin: None,
                prng_seed: to_binary("data").unwrap(),
                config: Some(InitConfig {
                    public_total_supply: Some(true),
                    ..Default::default()
                }),
                supported_denoms: None,
            },
            &[],
            "voting token",
            None,
        )
        .unwrap();

    let voting_contract_info = instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20token_info.address.clone().to_string(),
                code_hash: snip20token_info.code_hash.clone(),
                staking_contract: StakingInfo::New {
                    staking_code_id: staking_info.code_id,
                    staking_code_hash: staking_info.code_hash.clone(),
                    unstaking_duration: None,
                    label: "voting".into(),
                },
            },
            active_threshold: None,
            query_auth: Some(RawContract::new(
                &query_auth_info.address.into(),
                &query_auth_info.code_hash,
            )),
            dao_code_hash: "".into(),
        },
    );

    let snip20token_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::TokenContract {},
        )
        .unwrap();
    let staking_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::StakingContract {},
        )
        .unwrap();

    let mut token_info = TokenInfo {
        name: "".into(),
        symbol: "".into(),
        decimals: 0,
        total_supply: None,
    };
    let res: QueryAnswer = app
        .wrap()
        .query_wasm_smart(
            snip20token_info.code_hash.clone(),
            snip20token_info.addr.clone(),
            &snip20_reference_impl::msg::QueryMsg::TokenInfo {},
        )
        .unwrap();
    if let QueryAnswer::TokenInfo {
        name,
        symbol,
        decimals,
        total_supply,
    } = res
    {
        token_info.name = name;
        token_info.symbol = symbol;
        token_info.decimals = decimals;
        token_info.total_supply = total_supply;
    }
    assert_eq!(
        token_info,
        TokenInfo {
            name: "DAO DAO".to_string(),
            symbol: "DAO".to_string(),
            decimals: 3,
            total_supply: Some(Uint128::from(2u64))
        }
    );

    let mut minter = Minters { minters: vec![] };
    let res: QueryAnswer = app
        .wrap()
        .query_wasm_smart(
            snip20token_info.code_hash.clone(),
            snip20token_info.addr.clone(),
            &snip20_reference_impl::msg::QueryMsg::Minters {},
        )
        .unwrap();
    if let QueryAnswer::Minters { minters } = res {
        minter.minters = minters;
    }

    assert_eq!(minter, Minters { minters: vec![] });

    // Expect 0 as creator has not staked
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key_creator.clone(),
                    address: CREATOR_ADDR.into(),
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
    let dao_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key_dao.clone(),
                    address: DAO_ADDR.into(),
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
        staking_info.addr,
        staking_info.code_hash,
        ContractInfo {
            address: snip20token_info.addr.clone(),
            code_hash: snip20token_info.code_hash.clone(),
        },
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key_creator.clone(),
            address: CREATOR_ADDR.into(),
        },
        1,
    );
    app.update_block(next_block);

    // Expect 1 as creator has now staked 1
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key_creator.clone(),
                    address: CREATOR_ADDR.into(),
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
            voting_contract_info.code_hash,
            voting_contract_info.address,
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
fn test_existing_snip20_existing_staking() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_info = app.store_code(staking_contract());

    let query_auth_info = instantiate_query_auth(&mut app);

    let viewing_key_creator = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.address.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        CREATOR_ADDR,
    );
    let viewing_key_dao = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.address.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        DAO_ADDR,
    );

    let snip20token_info = app
        .instantiate_contract(
            snip20_info.clone(),
            Addr::unchecked(CREATOR_ADDR),
            &snip20_reference_impl::msg::InstantiateMsg {
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 3,
                initial_balances: Some(vec![Snip20InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::from(2u64),
                }]),
                admin: None,
                prng_seed: to_binary("data").unwrap(),
                config: Some(InitConfig {
                    public_total_supply: Some(true),
                    ..Default::default()
                }),
                supported_denoms: None,
            },
            &[],
            "voting token",
            None,
        )
        .unwrap();

    let voting_contract_info = instantiate_voting(
        &mut app,
        voting_info.clone(),
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20token_info.address.clone().to_string(),
                code_hash: snip20token_info.code_hash.clone(),
                staking_contract: StakingInfo::New {
                    staking_code_id: staking_info.code_id,
                    staking_code_hash: staking_info.code_hash.clone(),
                    unstaking_duration: None,
                    label: "staking".into(),
                },
            },
            active_threshold: None,
            dao_code_hash: "".into(),
            query_auth: Some(RawContract::new(
                &query_auth_info.address.clone().into(),
                &query_auth_info.code_hash.clone(),
            )),
        },
    );

    let snip20token_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::TokenContract {},
        )
        .unwrap();
    // We'll use this for our valid existing contract
    let staking_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::StakingContract {},
        )
        .unwrap();

    let mut token_info = TokenInfo {
        name: "".into(),
        symbol: "".into(),
        decimals: 0,
        total_supply: None,
    };
    let res: QueryAnswer = app
        .wrap()
        .query_wasm_smart(
            snip20token_info.code_hash.clone(),
            snip20token_info.addr.clone(),
            &snip20_reference_impl::msg::QueryMsg::TokenInfo {},
        )
        .unwrap();
    if let QueryAnswer::TokenInfo {
        name,
        symbol,
        decimals,
        total_supply,
    } = res
    {
        token_info.name = name;
        token_info.symbol = symbol;
        token_info.decimals = decimals;
        token_info.total_supply = total_supply;
    }
    assert_eq!(
        token_info,
        TokenInfo {
            name: "DAO DAO".to_string(),
            symbol: "DAO".to_string(),
            decimals: 3,
            total_supply: Some(Uint128::from(2u64))
        }
    );

    let voting_contract_info = instantiate_voting(
        &mut app,
        voting_info.clone(),
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20token_info.addr.clone().to_string(),
                code_hash: snip20token_info.code_hash.clone(),
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_info.addr.clone().to_string(),
                    staking_contract_code_hash: staking_info.code_hash.clone(),
                },
            },
            active_threshold: None,
            query_auth: Some(RawContract::new(
                &query_auth_info.address.clone().into(),
                &query_auth_info.code_hash.clone(),
            )),
            dao_code_hash: "".into(),
        },
    );

    // Expect 0 as creator has not staked
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key_creator.clone(),
                    address: CREATOR_ADDR.into(),
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
    let dao_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key_dao.clone(),
                    address: DAO_ADDR.into(),
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
        staking_info.addr.clone(),
        staking_info.code_hash.clone(),
        ContractInfo {
            address: snip20token_info.addr.clone(),
            code_hash: snip20token_info.code_hash.clone(),
        },
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key_creator.clone(),
            address: CREATOR_ADDR.into(),
        },
        1,
    );
    app.update_block(next_block);

    // Expect 1 as creator has now staked 1
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key_creator,
                    address: CREATOR_ADDR.into(),
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
            voting_contract_info.code_hash,
            voting_contract_info.address,
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
    let different_token = app
        .instantiate_contract(
            snip20_info,
            Addr::unchecked(CREATOR_ADDR),
            &snip20_reference_impl::msg::InstantiateMsg {
                name: "DAO DAO MISMATCH".to_string(),
                symbol: "DAOM".to_string(),
                decimals: 3,
                initial_balances: Some(vec![Snip20InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::from(2u64),
                }]),
                admin: None,
                prng_seed: to_binary("data").unwrap(),
                config: Some(InitConfig {
                    public_total_supply: Some(true),
                    ..Default::default()
                }),
                supported_denoms: None,
            },
            &[],
            "voting token",
            None,
        )
        .unwrap();

    // Expect error as the token address does not match the staking address token address
    app.instantiate_contract(
        voting_info,
        Addr::unchecked(DAO_ADDR),
        &InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: different_token.address.to_string(),
                code_hash: different_token.code_hash,
                staking_contract: StakingInfo::Existing {
                    staking_contract_address: staking_info.addr.to_string(),
                    staking_contract_code_hash: staking_info.code_hash,
                },
            },
            active_threshold: None,
            query_auth: Some(RawContract::new(
                &query_auth_info.address.clone().into(),
                &query_auth_info.code_hash.clone(),
            )),
            dao_code_hash: "".into(),
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
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_info = app.store_code(staking_contract());
    let query_auth_info = instantiate_query_auth(&mut app);

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.address.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        CREATOR_ADDR,
    );

    let snip20token_info = app
        .instantiate_contract(
            snip20_info,
            Addr::unchecked(CREATOR_ADDR),
            &snip20_reference_impl::msg::InstantiateMsg {
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 3,
                initial_balances: Some(vec![Snip20InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::from(2u64),
                }]),
                admin: None,
                prng_seed: to_binary("data").unwrap(),
                config: Some(InitConfig {
                    public_total_supply: Some(true),
                    ..Default::default()
                }),
                supported_denoms: None,
            },
            &[],
            "voting token",
            None,
        )
        .unwrap();

    let voting_contract_info = instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::Existing {
                address: snip20token_info.address.clone().to_string(),
                code_hash: snip20token_info.code_hash.clone(),
                staking_contract: StakingInfo::New {
                    staking_code_id: staking_info.code_id,
                    staking_code_hash: staking_info.code_hash.clone(),
                    unstaking_duration: None,
                    label: "staking".into(),
                },
            },
            active_threshold: None,
            dao_code_hash: "".into(),
            query_auth: Some(RawContract::new(
                &query_auth_info.address.to_string(),
                &query_auth_info.code_hash,
            )),
        },
    );

    let snip20token_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::TokenContract {},
        )
        .unwrap();
    let staking_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::StakingContract {},
        )
        .unwrap();

    // Expect 0 as creator has not staked
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: CREATOR_ADDR.into(),
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
        staking_info.addr.clone(),
        staking_info.code_hash.clone(),
        ContractInfo {
            address: snip20token_info.addr.clone(),
            code_hash: snip20token_info.code_hash.clone(),
        },
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        1,
    );
    app.update_block(next_block);

    // Expect 1 as creator has now staked 1
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: CREATOR_ADDR.into(),
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
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
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

    // Stake another 1 token as creator
    stake_tokens(
        &mut app,
        staking_info.addr.clone(),
        staking_info.code_hash.clone(),
        ContractInfo {
            address: snip20token_info.addr.clone(),
            code_hash: snip20token_info.code_hash.clone(),
        },
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        1,
    );
    app.update_block(next_block);

    // Expect 2 as creator has now staked 2
    let creator_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::VotingPowerAtHeight {
                auth: Auth::ViewingKey {
                    key: viewing_key.clone(),
                    address: CREATOR_ADDR.into(),
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

    // Expect 2 as we have now staked 2
    let total_voting_power: VotingPowerAtHeightResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
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

    // // Check we can query history
    // let creator_voting_power: VotingPowerAtHeightResponse = app
    //     .wrap()
    //     .query_wasm_smart(
    //         voting_contract_info.code_hash.clone(),
    //         voting_contract_info.address.clone(),
    //         &QueryMsg::VotingPowerAtHeight {
    //             auth: Auth::ViewingKey {
    //                 key: viewing_key.clone(),
    //                 address: CREATOR_ADDR.into(),
    //             },
    //             height: Some(app.block_info().height - 1),
    //         },
    //     )
    //     .unwrap();

    // assert_eq!(
    //     creator_voting_power,
    //     VotingPowerAtHeightResponse {
    //         power: Uint128::new(1u128),
    //         height: app.block_info().height - 1,
    //     }
    // );

    // // Expect 1 at the old height prior to second stake
    // let total_voting_power: VotingPowerAtHeightResponse = app
    //     .wrap()
    //     .query_wasm_smart(
    //         voting_contract_info.code_hash,
    //         voting_contract_info.address,
    //         &QueryMsg::TotalPowerAtHeight {
    //             height: Some(app.block_info().height - 1),
    //         },
    //     )
    //     .unwrap();

    // assert_eq!(
    //     total_voting_power,
    //     VotingPowerAtHeightResponse {
    //         power: Uint128::new(1u128),
    //         height: app.block_info().height - 1,
    //     }
    // );
}

#[test]
fn test_active_threshold_absolute_count() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_contract_info = app.store_code(staking_contract());
    let query_auth_info = instantiate_query_auth(&mut app);

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.address.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        CREATOR_ADDR,
    );

    let voting_contract_info = instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::from(200u64),
                }],
                staking_code_id: staking_contract_info.code_id,
                staking_code_hash: staking_contract_info.code_hash,
                initial_dao_balance: Some(Uint128::from(100u64)),
                unstaking_duration: None,
            },
            active_threshold: Some(ActiveThreshold::AbsoluteCount {
                count: Uint128::new(100),
            }),
            dao_code_hash: "".into(),
            query_auth: Some(RawContract::new(
                &query_auth_info.address.to_string(),
                &query_auth_info.code_hash,
            )),
        },
    );

    let snip20token_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::TokenContract {},
        )
        .unwrap();
    let staking_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::StakingContract {},
        )
        .unwrap();

    // Not active as none staked
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(!is_active.active);

    // Stake 100 token as creator
    stake_tokens(
        &mut app,
        staking_info.addr,
        staking_info.code_hash,
        ContractInfo {
            address: snip20token_info.addr,
            code_hash: snip20token_info.code_hash,
        },
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key,
            address: CREATOR_ADDR.into(),
        },
        100,
    );
    app.update_block(next_block);

    // Active as enough staked
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash,
            voting_contract_info.address,
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(is_active.active);
}

#[test]
fn test_active_threshold_percent() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_contract_info = app.store_code(staking_contract());
    let query_auth_info = instantiate_query_auth(&mut app);

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.address.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        CREATOR_ADDR,
    );

    let voting_contract_info = instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::from(200u64),
                }],
                unstaking_duration: None,
                staking_code_id: staking_contract_info.code_id,
                staking_code_hash: staking_contract_info.code_hash,
                initial_dao_balance: Some(Uint128::from(100u64)),
            },
            active_threshold: Some(ActiveThreshold::Percentage {
                percent: Decimal::percent(20),
            }),
            dao_code_hash: "".into(),
            query_auth: Some(RawContract::new(
                &query_auth_info.address.to_string(),
                &query_auth_info.code_hash,
            )),
        },
    );

    let snip20token_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::TokenContract {},
        )
        .unwrap();
    let staking_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::StakingContract {},
        )
        .unwrap();

    // Not active as none staked
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(!is_active.active);

    // Stake 60 token as creator, now active
    stake_tokens(
        &mut app,
        staking_info.addr,
        staking_info.code_hash,
        ContractInfo {
            address: snip20token_info.addr,
            code_hash: snip20token_info.code_hash,
        },
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key,
            address: CREATOR_ADDR.into(),
        },
        60,
    );
    app.update_block(next_block);

    // Active as enough staked
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash,
            voting_contract_info.address,
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(is_active.active);
}

#[test]
fn test_active_threshold_percent_rounds_up() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_contract_info = app.store_code(staking_contract());
    let query_auth_info = instantiate_query_auth(&mut app);

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth_info.address.clone(),
            code_hash: query_auth_info.code_hash.clone(),
        },
        CREATOR_ADDR,
    );

    let voting_contract_info = instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::from(200u64),
                }],
                unstaking_duration: None,
                staking_code_id: staking_contract_info.code_id,
                staking_code_hash: staking_contract_info.code_hash,
                initial_dao_balance: Some(Uint128::from(100u64)),
            },
            active_threshold: Some(ActiveThreshold::Percentage {
                percent: Decimal::percent(50),
            }),
            dao_code_hash: "".into(),
            query_auth: Some(RawContract::new(
                &query_auth_info.address.to_string(),
                &query_auth_info.code_hash,
            )),
        },
    );

    let snip20token_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.address.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::TokenContract {},
        )
        .unwrap();
    let staking_info: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::StakingContract {},
        )
        .unwrap();

    // Not active as none staked
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(!is_active.active);

    // Stake 2 token as creator, should not be active.
    stake_tokens(
        &mut app,
        staking_info.addr.clone(),
        staking_info.code_hash.clone(),
        ContractInfo {
            address: snip20token_info.addr.clone(),
            code_hash: snip20token_info.code_hash.clone(),
        },
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        2,
    );
    app.update_block(next_block);

    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(!is_active.active);

    // Stake 1 more token as creator, should now be active.
    stake_tokens(
        &mut app,
        staking_info.addr,
        staking_info.code_hash,
        ContractInfo {
            address: snip20token_info.addr.clone(),
            code_hash: snip20token_info.code_hash.clone(),
        },
        CREATOR_ADDR,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        1,
    );
    app.update_block(next_block);
}

#[test]
fn test_active_threshold_none() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_contract_info = app.store_code(staking_contract());

    let voting_contract_info = instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::from(200u64),
                }],
                unstaking_duration: None,
                staking_code_id: staking_contract_info.code_id,
                staking_code_hash: staking_contract_info.code_hash,
                initial_dao_balance: Some(Uint128::from(100u64)),
            },
            active_threshold: None,
            query_auth: None,
            dao_code_hash: "".into(),
        },
    );

    // Active as no threshold
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash,
            voting_contract_info.address,
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(is_active.active);
}

#[test]
fn test_update_active_threshold() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_contract_info = app.store_code(staking_contract());

    let voting_contract_info = instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::from(200u64),
                }],
                unstaking_duration: None,
                staking_code_id: staking_contract_info.code_id,
                staking_code_hash: staking_contract_info.code_hash,
                initial_dao_balance: Some(Uint128::from(100u64)),
            },
            active_threshold: None,
            query_auth: None,
            dao_code_hash: "".into(),
        },
    );

    let resp: ActiveThresholdResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash.clone(),
            voting_contract_info.address.clone(),
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
        &voting_contract_info.clone(),
        &msg,
        &[],
    )
    .unwrap_err();

    // Expect success as sender is the DAO
    app.execute_contract(
        Addr::unchecked(DAO_ADDR),
        &voting_contract_info.clone(),
        &msg,
        &[],
    )
    .unwrap();

    let resp: ActiveThresholdResponse = app
        .wrap()
        .query_wasm_smart(
            voting_contract_info.code_hash,
            voting_contract_info.address,
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
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_contract_info = app.store_code(staking_contract());

    instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::from(200u64),
                }],
                unstaking_duration: None,
                staking_code_id: staking_contract_info.code_id,
                staking_code_hash: staking_contract_info.code_hash,
                initial_dao_balance: Some(Uint128::from(100u64)),
            },
            active_threshold: Some(ActiveThreshold::Percentage {
                percent: Decimal::percent(120),
            }),
            query_auth: None,
            dao_code_hash: "".into(),
        },
    );
}

#[test]
#[should_panic(expected = "Active threshold percentage must be greater than 0 and less than 1")]
fn test_active_threshold_percentage_lte_0() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_contract_info = app.store_code(staking_contract());

    instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::from(200u64),
                }],
                unstaking_duration: None,
                staking_code_id: staking_contract_info.code_id,
                staking_code_hash: staking_contract_info.code_hash,
                initial_dao_balance: Some(Uint128::from(100u64)),
            },
            active_threshold: Some(ActiveThreshold::Percentage {
                percent: Decimal::percent(0),
            }),
            dao_code_hash: "".into(),
            query_auth: None,
        },
    );
}

#[test]
#[should_panic(expected = "Absolute count threshold cannot be greater than the total token supply")]
fn test_active_threshold_absolute_count_invalid() {
    let mut app = App::default();
    let snip20_info = app.store_code(snip20_contract());
    let voting_info = app.store_code(staked_balance_voting_contract());
    let staking_contract_info = app.store_code(staking_contract());

    instantiate_voting(
        &mut app,
        voting_info,
        InstantiateMsg {
            token_info: crate::msg::Snip20TokenInfo::New {
                code_id: snip20_info.code_id,
                code_hash: snip20_info.code_hash,
                name: "DAO DAO".to_string(),
                symbol: "DAO".to_string(),
                decimals: 6,
                initial_balances: vec![InitialBalance {
                    address: CREATOR_ADDR.to_string(),
                    amount: Uint128::from(200u64),
                }],
                unstaking_duration: None,
                staking_code_id: staking_contract_info.code_id,
                staking_code_hash: staking_contract_info.code_hash,
                initial_dao_balance: Some(Uint128::from(100u64)),
            },
            active_threshold: Some(ActiveThreshold::AbsoluteCount {
                count: Uint128::new(10000),
            }),
            query_auth: None,
            dao_code_hash: "".into(),
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
