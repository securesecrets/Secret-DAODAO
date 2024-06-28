use anyhow::Result as AnyResult;
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{from_binary, to_binary, Addr, ContractInfo, Empty, MessageInfo, Uint128};
use cw_ownable::{Action, Ownership, OwnershipError};
use dao_voting::duration::UnstakingDurationError;
use secret_cw_controllers::{Claim, ClaimsResponse};
use secret_multi_test::{next_block, App, AppResponse, Contract, ContractWrapper, Executor};
use secret_utils::Duration;
use secret_utils::Expiration::AtHeight;
use shade_protocol::basic_staking::Auth;
use snip20_reference_impl::msg::InitialBalance;
use std::borrow::BorrowMut;

use crate::msg::{
    ExecuteMsg, QueryMsg, ReceiveMsg, StakedBalanceAtHeightResponse, TotalStakedAtHeightResponse,
};
use crate::state::{Config, MAX_CLAIMS};
use crate::ContractError;

const ADDR1: &str = "addr0001";
const ADDR2: &str = "addr0002";
const ADDR3: &str = "addr0003";
const ADDR4: &str = "addr0004";
const OWNER: &str = "owner";

fn contract_staking() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_migrate(crate::contract::migrate);
    Box::new(contract)
}

fn contract_snip20() -> Box<dyn Contract<Empty>> {
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

fn mock_app() -> App {
    App::default()
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
    if let shade_protocol::contract_interfaces::query_auth::ExecuteAnswer::CreateViewingKey {
        key,
    } = data
    {
        viewing_key = key;
    };
    viewing_key
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
        Addr::unchecked(ADDR1),
        &msg,
        &[],
        "query_auth",
        None,
    )
    .unwrap()
}

fn instantiate_snip20(app: &mut App, initial_balances: Vec<InitialBalance>) -> ContractInfo {
    let snip20_info = app.store_code(contract_snip20());
    let msg = snip20_reference_impl::msg::InstantiateMsg {
        name: String::from("Test"),
        symbol: String::from("TEST"),
        decimals: 6,
        initial_balances: Some(initial_balances),
        admin: None,
        prng_seed: to_binary("seed").unwrap(),
        config: None,
        supported_denoms: None,
    };

    app.instantiate_contract(
        snip20_info,
        Addr::unchecked(ADDR1),
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
    let staking_info = app.store_code(contract_staking());
    let msg = crate::msg::InstantiateMsg {
        owner: Some(OWNER.to_string()),
        token_address: snip20.to_string(),
        unstaking_duration,
        token_code_hash: Some(snip20_code_hash),
        query_auth: query_auth.into(),
    };
    app.instantiate_contract(
        staking_info,
        Addr::unchecked(ADDR1),
        &msg,
        &[],
        "staking",
        Some("admin".to_string()),
    )
    .unwrap()
}

fn setup_test_case(
    app: &mut App,
    initial_balances: Vec<InitialBalance>,
    unstaking_duration: Option<Duration>,
) -> (ContractInfo, ContractInfo, ContractInfo) {
    // Instantiate snip20 contract
    let snip20_info = instantiate_snip20(app, initial_balances);
    let query_auth_info = instantiate_query_auth(app);
    app.update_block(next_block);
    let query_auth = shade_protocol::Contract {
        address: query_auth_info.clone().address,
        code_hash: query_auth_info.clone().code_hash,
    };
    // Instantiate staking contract
    let staking_info = instantiate_staking(
        app,
        snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        unstaking_duration,
        query_auth,
    );
    app.update_block(next_block);
    (staking_info, snip20_info, query_auth_info)
}

fn query_staked_balance(
    app: &App,
    contract_addr: String,
    code_hash: String,
    auth: Auth,
) -> Uint128 {
    let msg = QueryMsg::StakedBalanceAtHeight { auth, height: None };
    let result: StakedBalanceAtHeightResponse = app
        .wrap()
        .query_wasm_smart(code_hash, contract_addr, &msg)
        .unwrap();
    result.balance
}

fn query_config<T: Into<String>, C: Into<String>>(
    app: &App,
    contract_addr: T,
    code_hash: C,
) -> Config {
    let msg = QueryMsg::GetConfig {};
    app.wrap()
        .query_wasm_smart(code_hash, contract_addr, &msg)
        .unwrap()
}

fn query_owner<T: Into<String>, C: Into<String>>(
    app: &App,
    contract_addr: T,
    code_hash: C,
) -> Ownership<Addr> {
    app.wrap()
        .query_wasm_smart(code_hash, contract_addr, &QueryMsg::Ownership {})
        .unwrap()
}

fn query_total_staked<T: Into<String>, C: Into<String>>(
    app: &App,
    contract_addr: T,
    code_hash: C,
) -> Uint128 {
    let msg = QueryMsg::TotalStakedAtHeight { height: None };
    let result: TotalStakedAtHeightResponse = app
        .wrap()
        .query_wasm_smart(code_hash, contract_addr, &msg)
        .unwrap();
    result.total
}

fn query_claims<T: Into<String>, C: Into<String>, Q: Into<Auth>>(
    app: &App,
    contract_addr: T,
    code_hash: C,
    auth: Q,
) -> Vec<Claim> {
    let msg = QueryMsg::Claims { auth: auth.into() };
    let result: ClaimsResponse = app
        .wrap()
        .query_wasm_smart(code_hash, contract_addr, &msg)
        .unwrap();
    result.claims
}

fn stake_tokens(
    app: &mut App,
    staking_addr: &Addr,
    staking_code_hash: String,
    snip20_addr: &Addr,
    snip20_code_hash: String,
    info: MessageInfo,
    amount: Uint128,
    auth: Auth,
) -> AnyResult<AppResponse> {
    let msg = secret_toolkit::snip20::HandleMsg::Send {
        amount,
        msg: Some(to_binary(&ReceiveMsg::Stake { auth }).unwrap()),
        recipient: staking_addr.to_string(),
        recipient_code_hash: Some(staking_code_hash),
        memo: None,
        padding: None,
    };
    app.execute_contract(
        info.sender,
        &ContractInfo {
            address: snip20_addr.clone(),
            code_hash: snip20_code_hash,
        },
        &msg,
        &[],
    )
}

fn update_config(
    app: &mut App,
    staking_addr: &Addr,
    staking_code_hash: String,
    info: MessageInfo,
    duration: Option<Duration>,
) -> AnyResult<AppResponse> {
    let msg = ExecuteMsg::UpdateConfig { duration };
    app.execute_contract(
        info.sender,
        &ContractInfo {
            address: staking_addr.clone(),
            code_hash: staking_code_hash,
        },
        &msg,
        &[],
    )
}

fn unstake_tokens(
    app: &mut App,
    staking_addr: &Addr,
    staking_code_hash: String,
    info: MessageInfo,
    amount: Uint128,
    auth: Auth,
) -> AnyResult<AppResponse> {
    let msg = ExecuteMsg::Unstake { auth, amount };
    app.execute_contract(
        info.sender,
        &ContractInfo {
            address: staking_addr.clone(),
            code_hash: staking_code_hash,
        },
        &msg,
        &[],
    )
}

fn claim_tokens(
    app: &mut App,
    staking_addr: &Addr,
    staking_code_hash: String,
    info: MessageInfo,
) -> AnyResult<AppResponse> {
    let msg = ExecuteMsg::Claim {};
    app.execute_contract(
        info.sender,
        &ContractInfo {
            address: staking_addr.clone(),
            code_hash: staking_code_hash,
        },
        &msg,
        &[],
    )
}

#[test]
#[should_panic(expected = "Invalid unstaking duration, unstaking duration cannot be 0")]
fn test_instantiate_invalid_unstaking_duration() {
    let mut app = mock_app();
    let amount1 = Uint128::from(100u128);
    let _token_address = Addr::unchecked("token_address");
    let initial_balances = vec![InitialBalance {
        address: ADDR1.to_string(),
        amount: amount1,
    }];
    let (_staking_addr, _cw20_addr, _) =
        setup_test_case(&mut app, initial_balances, Some(Duration::Height(0)));
}

#[test]
#[should_panic(expected = "ContractData not found")]
fn test_instantiate_with_non_cw20_token() {
    let app = &mut mock_app();
    let query_auth = instantiate_query_auth(app);
    instantiate_staking(
        app,
        Addr::unchecked("ekez"),
        "aaas".to_string(),
        None,
        shade_protocol::Contract {
            address: query_auth.address,
            code_hash: query_auth.code_hash,
        },
    );
}

#[test]
fn test_update_config() {
    let mut app = mock_app();
    let amount1 = Uint128::from(100u128);
    let initial_balances = vec![InitialBalance {
        address: ADDR1.to_string(),
        amount: amount1,
    }];
    let (staking_info, _snip20_info, _query_auth_info) =
        setup_test_case(&mut app, initial_balances, None);

    // Owner can update configuration.
    let info = mock_info(OWNER, &[]);
    update_config(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info,
        Some(Duration::Height(1234)),
    )
    .unwrap();
    let config = query_config(
        &app,
        staking_info.address.clone(),
        staking_info.code_hash.clone(),
    );
    assert_eq!(config.unstaking_duration, Some(Duration::Height(1234)));

    // Non owner may not update configuration.
    let info = mock_info(ADDR1, &[]);
    let err: ContractError = update_config(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info,
        None,
    )
    .unwrap_err()
    .downcast()
    .unwrap();
    assert_eq!(err, ContractError::Ownership(OwnershipError::NotOwner));

    // Zero durations not allowed.
    let info = mock_info(OWNER, &[]);
    let err: ContractError = update_config(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info,
        Some(Duration::Height(0)),
    )
    .unwrap_err()
    .downcast()
    .unwrap();
    assert_eq!(
        err,
        ContractError::UnstakingDurationError(UnstakingDurationError::InvalidUnstakingDuration {})
    );

    let info = mock_info(OWNER, &[]);
    let err: ContractError = update_config(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info,
        Some(Duration::Time(0)),
    )
    .unwrap_err()
    .downcast()
    .unwrap();
    assert_eq!(
        err,
        ContractError::UnstakingDurationError(UnstakingDurationError::InvalidUnstakingDuration {})
    );
}

#[test]
fn test_staking() {
    let _deps = mock_dependencies();

    let mut app = mock_app();
    let amount1 = Uint128::from(100u128);
    let _token_address = Addr::unchecked("token_address");
    let initial_balances = vec![InitialBalance {
        address: ADDR1.to_string(),
        amount: amount1,
    }];
    let (staking_info, snip20_info, query_auth_info) =
        setup_test_case(&mut app, initial_balances, None);

    let info = mock_info(ADDR1, &[]);
    let viewing_key_user1 = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());
    let _env = mock_env();

    // Successful bond
    let amount = Uint128::new(50);
    stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        amount,
        Auth::ViewingKey {
            key: viewing_key_user1.clone(),
            address: info.clone().sender.into_string(),
        },
    )
    .unwrap();

    let viewing_key_user1 = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user1.clone(),
                address: ADDR1.to_string().clone()
            }
        ),
        Uint128::from(50u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(50u128)
    );

    // Can't transfer bonded amount
    let msg = secret_toolkit::snip20::HandleMsg::Transfer {
        recipient: ADDR2.to_string(),
        amount: Uint128::from(51u128),
        memo: None,
        padding: None,
    };
    let _err = app
        .borrow_mut()
        .execute_contract(info.sender.clone(), &snip20_info.clone(), &msg, &[])
        .unwrap_err();

    // Sucessful transfer of unbonded amount
    let msg = secret_toolkit::snip20::HandleMsg::Transfer {
        recipient: ADDR2.to_string(),
        amount: Uint128::from(20u128),
        memo: None,
        padding: None,
    };
    let _res = app
        .borrow_mut()
        .execute_contract(info.sender.clone(), &snip20_info.clone(), &msg, &[])
        .unwrap();

    // Addr 2 successful bond
    let info = mock_info(ADDR2, &[]);
    let viewing_key_user2 = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

    stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        Uint128::new(20),
        Auth::ViewingKey {
            key: viewing_key_user2.clone(),
            address: info.clone().sender.into_string(),
        },
    )
    .unwrap();

    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user2.clone(),
                address: ADDR2.to_string().clone()
            }
        ),
        Uint128::from(20u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(70u128)
    );

    // Can't unstake more than you have staked
    let info = mock_info(ADDR2, &[]);
    let _err = unstake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
        Uint128::new(100),
        Auth::ViewingKey {
            key: viewing_key_user2.clone(),
            address: info.clone().sender.into_string(),
        },
    )
    .unwrap_err();

    // Successful unstake
    let _res = unstake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
        Uint128::new(10),
        Auth::ViewingKey {
            key: viewing_key_user2.clone(),
            address: info.clone().sender.into_string(),
        },
    )
    .unwrap();
    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user2.clone(),
                address: ADDR2.to_string().clone()
            }
        ),
        Uint128::from(10u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(60u128)
    );

    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user1.clone(),
                address: ADDR1.to_string().clone()
            }
        ),
        Uint128::from(50u128)
    );
}

#[test]
fn text_max_claims() {
    let mut app = mock_app();
    let amount1 = Uint128::from(MAX_CLAIMS + 1);
    let unstaking_blocks = 1u64;
    let _token_address = Addr::unchecked("token_address");
    let initial_balances = vec![InitialBalance {
        address: ADDR1.to_string(),
        amount: amount1,
    }];
    let (staking_info, snip20_info, query_auth) = setup_test_case(
        &mut app,
        initial_balances,
        Some(Duration::Height(unstaking_blocks)),
    );

    let info = mock_info(ADDR1, &[]);
    let viewing_key_user1 = create_viewing_key(&mut app, query_auth.clone(), info.clone());
    stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        amount1,
        Auth::ViewingKey {
            key: viewing_key_user1.clone(),
            address: info.clone().sender.into_string(),
        },
    )
    .unwrap();

    // Create the max number of claims
    for _ in 0..MAX_CLAIMS {
        unstake_tokens(
            &mut app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            info.clone(),
            Uint128::new(1),
            Auth::ViewingKey {
                key: viewing_key_user1.clone(),
                address: info.clone().sender.into_string(),
            },
        )
        .unwrap();
    }

    // Additional unstaking attempts ought to fail.
    unstake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
        Uint128::new(1),
        Auth::ViewingKey {
            key: viewing_key_user1.clone(),
            address: info.clone().sender.into_string(),
        },
    )
    .unwrap_err();

    // Clear out the claims list.
    app.update_block(next_block);
    claim_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
    )
    .unwrap();

    // Unstaking now allowed again.
    unstake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
        Uint128::new(1),
        Auth::ViewingKey {
            key: viewing_key_user1.clone(),
            address: info.clone().sender.into_string(),
        },
    )
    .unwrap();
    app.update_block(next_block);
    claim_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
    )
    .unwrap();
}

#[test]
fn test_unstaking_with_claims() {
    let _deps = mock_dependencies();

    let mut app = mock_app();
    let amount1 = Uint128::from(100u128);
    let unstaking_blocks = 10u64;
    let _token_address = Addr::unchecked("token_address");
    let initial_balances = vec![InitialBalance {
        address: ADDR1.to_string(),
        amount: amount1,
    }];
    let (staking_info, snip20_info, query_auth_info) = setup_test_case(
        &mut app,
        initial_balances,
        Some(Duration::Height(unstaking_blocks)),
    );

    let info = mock_info(ADDR1, &[]);
    let viewing_key_user1 = create_viewing_key(&mut app, query_auth_info, info.clone());

    // Successful bond
    let _res = stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        Uint128::new(50),
        Auth::ViewingKey {
            key: viewing_key_user1.clone(),
            address: info.clone().sender.into_string(),
        },
    )
    .unwrap();
    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user1.clone(),
                address: ADDR1.to_string().clone()
            }
        ),
        Uint128::from(50u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(50u128)
    );

    // Unstake
    let info = mock_info(ADDR1, &[]);
    let _res = unstake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
        Uint128::new(10),
        Auth::ViewingKey {
            key: viewing_key_user1.clone(),
            address: info.clone().sender.into_string(),
        },
    )
    .unwrap();
    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user1.clone(),
                address: ADDR1.to_string().clone()
            }
        ),
        Uint128::from(40u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(40u128)
    );

    // Cannot claim when nothing is available
    let info = mock_info(ADDR1, &[]);
    let _err: ContractError = claim_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info,
    )
    .unwrap_err()
    .downcast()
    .unwrap();
    assert_eq!(_err, ContractError::NothingToClaim {});

    // Successful claim
    app.update_block(|b| b.height += unstaking_blocks);
    let info = mock_info(ADDR1, &[]);
    let _res = claim_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info,
    )
    .unwrap();
    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user1.clone(),
                address: ADDR1.to_string().clone()
            }
        ),
        Uint128::from(40u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(40u128)
    );

    // Unstake and claim multiple
    let _info = mock_info(ADDR1, &[]);
    let info = mock_info(ADDR1, &[]);
    let _res = unstake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
        Uint128::new(5),
        Auth::ViewingKey {
            key: viewing_key_user1.clone(),
            address: info.clone().sender.into_string(),
        },
    )
    .unwrap();
    app.update_block(next_block);

    let _info = mock_info(ADDR1, &[]);
    let info = mock_info(ADDR1, &[]);
    let _res = unstake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
        Uint128::new(5),
        Auth::ViewingKey {
            key: viewing_key_user1.clone(),
            address: info.clone().sender.into_string(),
        },
    )
    .unwrap();
    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user1.clone(),
                address: ADDR1.to_string().clone()
            }
        ),
        Uint128::from(30u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(30u128)
    );

    app.update_block(|b| b.height += unstaking_blocks);
    let info = mock_info(ADDR1, &[]);
    let _res = claim_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info,
    )
    .unwrap();
    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user1.clone(),
                address: ADDR1.to_string().clone()
            }
        ),
        Uint128::from(30u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(30u128)
    );
}

#[test]
fn multiple_address_staking() {
    let amount1 = Uint128::from(100u128);
    let initial_balances = vec![
        InitialBalance {
            address: ADDR1.to_string(),
            amount: amount1,
        },
        InitialBalance {
            address: ADDR2.to_string(),
            amount: amount1,
        },
        InitialBalance {
            address: ADDR3.to_string(),
            amount: amount1,
        },
        InitialBalance {
            address: ADDR4.to_string(),
            amount: amount1,
        },
    ];
    let mut app = mock_app();
    let amount1 = Uint128::from(100u128);
    let unstaking_blocks = 10u64;
    let _token_address = Addr::unchecked("token_address");
    let (staking_info, snip20_info, query_auth_info) = setup_test_case(
        &mut app,
        initial_balances,
        Some(Duration::Height(unstaking_blocks)),
    );

    let info = mock_info(ADDR1, &[]);
    let viewing_key_user1 = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

    // Successful bond
    let _res = stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        amount1,
        Auth::ViewingKey {
            key: viewing_key_user1.clone(),
            address: ADDR1.to_string().clone(),
        },
    )
    .unwrap();
    app.update_block(next_block);

    let info = mock_info(ADDR2, &[]);
    let viewing_key_user2 = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

    // Successful bond
    let _res = stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        amount1,
        Auth::ViewingKey {
            key: viewing_key_user2.clone(),
            address: ADDR1.to_string().clone(),
        },
    )
    .unwrap();
    app.update_block(next_block);

    let info = mock_info(ADDR3, &[]);
    let viewing_key_user3 = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

    // Successful bond
    let _res = stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        amount1,
        Auth::ViewingKey {
            key: viewing_key_user3.clone(),
            address: ADDR1.to_string().clone(),
        },
    )
    .unwrap();
    app.update_block(next_block);

    let info = mock_info(ADDR4, &[]);
    let viewing_key_user4 = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

    // Successful bond
    let _res = stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        amount1,
        Auth::ViewingKey {
            key: viewing_key_user4.clone(),
            address: ADDR1.to_string().clone(),
        },
    )
    .unwrap();
    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user1.clone(),
                address: ADDR1.to_string().clone()
            }
        ),
        amount1
    );
    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user2.clone(),
                address: ADDR2.to_string().clone()
            }
        ),
        amount1
    );
    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user3.clone(),
                address: ADDR3.to_string().clone()
            }
        ),
        amount1
    );
    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user4.clone(),
                address: ADDR4.to_string().clone()
            }
        ),
        amount1
    );

    assert_eq!(
        query_total_staked(
            &app,
            staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        amount1.checked_mul(Uint128::new(4)).unwrap()
    );
}

#[test]
fn test_simple_unstaking_with_duration() {
    let _deps = mock_dependencies();

    let mut app = mock_app();
    let amount1 = Uint128::from(100u128);
    let _token_address = Addr::unchecked("token_address");
    let initial_balances = vec![
        InitialBalance {
            address: ADDR1.to_string(),
            amount: amount1,
        },
        InitialBalance {
            address: ADDR2.to_string(),
            amount: amount1,
        },
    ];
    let (staking_info, snip20_info, query_auth_info) =
        setup_test_case(&mut app, initial_balances, Some(Duration::Height(1)));

    // Bond Address 1
    let info = mock_info(ADDR1, &[]);
    let viewing_key_user1 = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

    let _env = mock_env();
    let amount = Uint128::new(100);
    stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        amount,
        Auth::ViewingKey {
            key: viewing_key_user1.clone(),
            address: ADDR1.to_string().clone(),
        },
    )
    .unwrap();

    // Bond Address 2
    let info = mock_info(ADDR2, &[]);
    let viewing_key_user2 = create_viewing_key(&mut app, query_auth_info.clone(), info.clone());

    let _env = mock_env();
    let amount = Uint128::new(100);
    let _res = stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        amount,
        Auth::ViewingKey {
            key: viewing_key_user2.clone(),
            address: ADDR1.to_string().clone(),
        },
    )
    .unwrap();
    app.update_block(next_block);
    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user1.clone(),
                address: ADDR1.to_string().clone()
            }
        ),
        Uint128::from(100u128)
    );

    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user2.clone(),
                address: ADDR2.to_string().clone()
            }
        ),
        Uint128::from(100u128)
    );

    // Unstake Addr1
    let info = mock_info(ADDR1, &[]);
    let _env = mock_env();
    let amount = Uint128::new(100);
    unstake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info,
        amount,
        Auth::ViewingKey {
            key: viewing_key_user1.clone(),
            address: ADDR1.to_string().clone(),
        },
    )
    .unwrap();
    // Unstake Addr2
    let info = mock_info(ADDR2, &[]);
    let _env = mock_env();
    let amount = Uint128::new(100);
    unstake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info,
        amount,
        Auth::ViewingKey {
            key: viewing_key_user2.clone(),
            address: ADDR1.to_string().clone(),
        },
    )
    .unwrap();
    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user1.clone(),
                address: ADDR1.to_string().clone()
            }
        ),
        Uint128::from(0u128)
    );

    assert_eq!(
        query_staked_balance(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user2.clone(),
                address: ADDR2.to_string().clone()
            }
        ),
        Uint128::from(0u128)
    );

    // Claim
    assert_eq!(
        query_claims(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user1.clone(),
                address: ADDR1.to_string().clone()
            }
        ),
        vec![Claim {
            amount: Uint128::new(100),
            release_at: AtHeight(12349)
        }]
    );
    assert_eq!(
        query_claims(
            &app,
            staking_info.address.to_string().clone(),
            staking_info.code_hash.clone(),
            Auth::ViewingKey {
                key: viewing_key_user2.clone(),
                address: ADDR2.to_string().clone()
            }
        ),
        vec![Claim {
            amount: Uint128::new(100),
            release_at: AtHeight(12349)
        }]
    );

    let info = mock_info(ADDR1, &[]);
    claim_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info,
    )
    .unwrap();

    let info = mock_info(ADDR2, &[]);
    claim_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info,
    )
    .unwrap();
}

#[test]
fn test_ownership_transfer() {
    let mut app = App::default();
    let snip20_info = instantiate_snip20(
        &mut app,
        vec![InitialBalance {
            address: OWNER.to_string(),
            amount: Uint128::from(1000u64),
        }],
    );
    let query_auth_info = instantiate_query_auth(&mut app);
    let staking_info = instantiate_staking(
        &mut app,
        snip20_info.address,
        snip20_info.code_hash,
        None,
        shade_protocol::Contract {
            address: query_auth_info.address,
            code_hash: query_auth_info.code_hash,
        },
    );

    app.execute_contract(
        Addr::unchecked(OWNER),
        &staking_info.clone(),
        &ExecuteMsg::UpdateOwnership(Action::TransferOwnership {
            new_owner: ADDR1.to_string(),
            expiry: None,
        }),
        &[],
    )
    .unwrap();

    let ownership = query_owner(
        &app,
        staking_info.address.to_string(),
        staking_info.code_hash.to_string(),
    );
    assert_eq!(
        ownership,
        Ownership::<Addr> {
            owner: Some(Addr::unchecked(OWNER)),
            pending_owner: Some(Addr::unchecked(ADDR1)),
            pending_expiry: None
        }
    );

    app.execute_contract(
        Addr::unchecked(ADDR1),
        &staking_info.clone(),
        &ExecuteMsg::UpdateOwnership(Action::AcceptOwnership),
        &[],
    )
    .unwrap();

    let ownership = query_owner(
        &app,
        staking_info.address.to_string(),
        staking_info.code_hash.to_string(),
    );
    assert_eq!(
        ownership,
        Ownership::<Addr> {
            owner: Some(Addr::unchecked(ADDR1)),
            pending_owner: None,
            pending_expiry: None
        }
    );
}
