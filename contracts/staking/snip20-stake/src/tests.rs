use anyhow::Result as AnyResult;
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{
    from_binary, to_binary, Addr, ContractInfo, Empty, MessageInfo, Uint128, WasmMsg,
};
use cw_ownable::{Action, Ownership, OwnershipError};
use dao_voting::duration::UnstakingDurationError;
use secret_cw_controllers::{Claim, ClaimsResponse};
use secret_multi_test::{next_block, App, AppResponse, Contract, ContractWrapper, Executor};
use secret_utils::Duration;
use secret_utils::Expiration::AtHeight;
use snip20_reference_impl::msg::InitialBalance;
use std::borrow::BorrowMut;

use crate::msg::{
    CreateViewingKeyResponse, ExecuteMsg, ListStakersResponse, MigrateMsg, QueryMsg, ReceiveMsg,
    StakedBalanceAtHeightResponse, StakedValueResponse, StakerBalanceResponse,
    TotalStakedAtHeightResponse, TotalValueResponse,
};
use crate::state::{Config, MAX_CLAIMS};
use crate::ContractError;

use cw20_stake_v1 as v1;

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

fn mock_app() -> App {
    App::default()
}

fn get_balance<T: Into<String>, U: Into<String>, C: Into<String>, K: Into<String>>(
    app: &App,
    contract_addr: T,
    code_hash: C,
    key: K,
    address: U,
) -> Uint128 {
    let msg = secret_toolkit::snip20::QueryMsg::Balance {
        address: address.into(),
        key: key.into(),
    };
    let result: secret_toolkit::snip20::query::Balance = app
        .wrap()
        .query_wasm_smart(code_hash, contract_addr, &msg)
        .unwrap();
    result.amount
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
) -> ContractInfo {
    let staking_info = app.store_code(contract_staking());
    let msg = crate::msg::InstantiateMsg {
        owner: Some(OWNER.to_string()),
        token_address: snip20.to_string(),
        unstaking_duration,
        token_code_hash: Some(snip20_code_hash),
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
) -> (ContractInfo, ContractInfo) {
    // Instantiate snip20 contract
    let snip20_info = instantiate_snip20(app, initial_balances);
    app.update_block(next_block);
    // Instantiate staking contract
    let staking_info = instantiate_staking(
        app,
        snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        unstaking_duration,
    );
    app.update_block(next_block);
    (staking_info, snip20_info)
}

fn query_staked_balance<T: Into<String>, U: Into<String>, C: Into<String>, K: Into<String>>(
    app: &App,
    contract_addr: T,
    code_hash: C,
    address: U,
    key: K,
) -> Uint128 {
    let msg = QueryMsg::StakedBalanceAtHeight {
        address: address.into(),
        height: None,
        key: key.into(),
    };
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

fn query_staked_value<T: Into<String>, U: Into<String>, C: Into<String>, K: Into<String>>(
    app: &App,
    contract_addr: T,
    code_hash: C,
    address: U,
    key: K,
) -> Uint128 {
    let msg = QueryMsg::StakedValue {
        address: address.into(),
        key: key.into(),
    };
    let result: StakedValueResponse = app
        .wrap()
        .query_wasm_smart(code_hash, contract_addr, &msg)
        .unwrap();
    result.value
}

fn query_total_value<T: Into<String>, C: Into<String>>(
    app: &App,
    contract_addr: T,
    code_hash: C,
) -> Uint128 {
    let msg = QueryMsg::TotalValue {};
    let result: TotalValueResponse = app
        .wrap()
        .query_wasm_smart(code_hash, contract_addr, &msg)
        .unwrap();
    result.total
}

fn query_claims<T: Into<String>, U: Into<String>, C: Into<String>, K: Into<String>>(
    app: &App,
    contract_addr: T,
    code_hash: C,
    address: U,
    key: K,
) -> Vec<Claim> {
    let msg = QueryMsg::Claims {
        address: address.into(),
        key: key.into(),
    };
    let result: ClaimsResponse = app
        .wrap()
        .query_wasm_smart(code_hash, contract_addr, &msg)
        .unwrap();
    result.claims
}

fn create_viewing_key_snip20(
    app: &mut App,
    snip20_addr: &Addr,
    snip20_code_hash: String,
    info: MessageInfo,
) -> AnyResult<AppResponse> {
    let msg = secret_toolkit::snip20::HandleMsg::CreateViewingKey {
        entropy: "entropy".to_string(),
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

fn create_viewing_key_snip20_staked(
    app: &mut App,
    snip20_staked_addr: &Addr,
    snip20_staked_code_hash: String,
    info: MessageInfo,
) -> AnyResult<AppResponse> {
    let msg = ExecuteMsg::CreateViewingKey {
        entropy: "entropy".to_string(),
        padding: None,
    };
    app.execute_contract(
        info.sender,
        &ContractInfo {
            address: snip20_staked_addr.clone(),
            code_hash: snip20_staked_code_hash,
        },
        &msg,
        &[],
    )
}

fn stake_tokens(
    app: &mut App,
    staking_addr: &Addr,
    staking_code_hash: String,
    snip20_addr: &Addr,
    snip20_code_hash: String,
    info: MessageInfo,
    amount: Uint128,
) -> AnyResult<AppResponse> {
    let msg = secret_toolkit::snip20::HandleMsg::Send {
        amount,
        msg: Some(to_binary(&ReceiveMsg::Stake {}).unwrap()),
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
) -> AnyResult<AppResponse> {
    let msg = ExecuteMsg::Unstake { amount };
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
    let (_staking_addr, _cw20_addr) =
        setup_test_case(&mut app, initial_balances, Some(Duration::Height(0)));
}

#[test]
#[should_panic(expected = "ContractData not found")]
fn test_instantiate_with_non_cw20_token() {
    let app = &mut mock_app();
    instantiate_staking(app, Addr::unchecked("ekez"), "aaas".to_string(), None);
}

#[test]
fn test_update_config() {
    let mut app = mock_app();
    let amount1 = Uint128::from(100u128);
    let initial_balances = vec![InitialBalance {
        address: ADDR1.to_string(),
        amount: amount1,
    }];
    let (staking_info, _snip20_info) = setup_test_case(&mut app, initial_balances, None);

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
        &staking_info.address.clone(),
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
    let (staking_info, snip20_info) = setup_test_case(&mut app, initial_balances, None);

    let info = mock_info(ADDR1, &[]);
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
    )
    .unwrap();

    let info2 = mock_info(ADDR2, &[]);

    let data_snip20_staked = create_viewing_key_snip20_staked(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
    )
    .unwrap();
    let viewing_key_snip20_staked: CreateViewingKeyResponse =
        from_binary(&data_snip20_staked.data.unwrap()).unwrap();

    let data_snip20_staked2 = create_viewing_key_snip20_staked(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info2.clone(),
    )
    .unwrap();
    let viewing_key_snip20_staked2: CreateViewingKeyResponse =
        from_binary(&data_snip20_staked2.data.unwrap()).unwrap();

    // Very important that this balances is not reflected until
    // the next block. This protects us from flash loan hostile
    // takeovers.
    // assert_eq!(
    //     query_staked_balance(
    //         &app,
    //         &staking_info.address.clone(),
    //         staking_info.code_hash.clone(),
    //         ADDR1.to_string(),
    //         viewing_key_snip20_staked.key.clone()
    //     ),
    //     Uint128::zero()
    // );

    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR1.to_string(),
            viewing_key_snip20_staked.key.clone()
        ),
        Uint128::from(50u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            &staking_info.address.clone(),
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
    stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        Uint128::new(20),
    )
    .unwrap();

    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR2,
            viewing_key_snip20_staked2.key.clone()
        ),
        Uint128::from(20u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            &staking_info.address.clone(),
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
    )
    .unwrap_err();

    // Successful unstake
    let _res = unstake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
        Uint128::new(10),
    )
    .unwrap();
    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR2,
            viewing_key_snip20_staked2.key.clone()
        ),
        Uint128::from(10u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(60u128)
    );

    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR1,
            viewing_key_snip20_staked.key.clone()
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
    let (staking_info, snip20_info) = setup_test_case(
        &mut app,
        initial_balances,
        Some(Duration::Height(unstaking_blocks)),
    );

    let info = mock_info(ADDR1, &[]);
    stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        amount1,
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
        &&staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
        Uint128::new(1),
    )
    .unwrap();
    app.update_block(next_block);
    claim_tokens(
        &mut app,
        &&staking_info.address.clone(),
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
    let (staking_info, snip20_info) = setup_test_case(
        &mut app,
        initial_balances,
        Some(Duration::Height(unstaking_blocks)),
    );

    let info = mock_info(ADDR1, &[]);
    let data_snip20_staked = create_viewing_key_snip20_staked(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
    )
    .unwrap();
    let viewing_key_snip20_staked: CreateViewingKeyResponse =
        from_binary(&data_snip20_staked.data.unwrap()).unwrap();

    // Successful bond
    let _res = stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info,
        Uint128::new(50),
    )
    .unwrap();
    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR1,
            viewing_key_snip20_staked.key.clone()
        ),
        Uint128::from(50u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            &staking_info.address.clone(),
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
        info,
        Uint128::new(10),
    )
    .unwrap();
    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR1,
            viewing_key_snip20_staked.key.clone()
        ),
        Uint128::from(40u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            &staking_info.address.clone(),
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
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR1,
            viewing_key_snip20_staked.key.clone()
        ),
        Uint128::from(40u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            &staking_info.address.clone(),
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
        info,
        Uint128::new(5),
    )
    .unwrap();
    app.update_block(next_block);

    let _info = mock_info(ADDR1, &[]);
    let info = mock_info(ADDR1, &[]);
    let _res = unstake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info,
        Uint128::new(5),
    )
    .unwrap();
    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR1,
            viewing_key_snip20_staked.key.clone()
        ),
        Uint128::from(30u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            &staking_info.address.clone(),
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
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR1,
            viewing_key_snip20_staked.key.clone()
        ),
        Uint128::from(30u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            &staking_info.address.clone(),
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
    let (staking_info, snip20_info) = setup_test_case(
        &mut app,
        initial_balances,
        Some(Duration::Height(unstaking_blocks)),
    );

    let info = mock_info(ADDR1, &[]);
    // Successful bond
    let _res = stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        amount1,
    )
    .unwrap();
    app.update_block(next_block);
    let data_snip20_staked1 = create_viewing_key_snip20_staked(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
    )
    .unwrap();
    let viewing_key_snip20_staked1: CreateViewingKeyResponse =
        from_binary(&data_snip20_staked1.data.unwrap()).unwrap();

    let info = mock_info(ADDR2, &[]);
    // Successful bond
    let _res = stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        amount1,
    )
    .unwrap();
    app.update_block(next_block);
    let data_snip20_staked2 = create_viewing_key_snip20_staked(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
    )
    .unwrap();
    let viewing_key_snip20_staked2: CreateViewingKeyResponse =
        from_binary(&data_snip20_staked2.data.unwrap()).unwrap();

    let info = mock_info(ADDR3, &[]);
    // Successful bond
    let _res = stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        amount1,
    )
    .unwrap();
    app.update_block(next_block);
    let data_snip20_staked3 = create_viewing_key_snip20_staked(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
    )
    .unwrap();
    let viewing_key_snip20_staked3: CreateViewingKeyResponse =
        from_binary(&data_snip20_staked3.data.unwrap()).unwrap();

    let info = mock_info(ADDR4, &[]);
    // Successful bond
    let _res = stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info.clone(),
        amount1,
    )
    .unwrap();
    app.update_block(next_block);
    let data_snip20_staked4 = create_viewing_key_snip20_staked(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
    )
    .unwrap();
    let viewing_key_snip20_staked4: CreateViewingKeyResponse =
        from_binary(&data_snip20_staked4.data.unwrap()).unwrap();

    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR1,
            viewing_key_snip20_staked1.key
        ),
        amount1
    );
    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR2,
            viewing_key_snip20_staked2.key
        ),
        amount1
    );
    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR3,
            viewing_key_snip20_staked3.key
        ),
        amount1
    );
    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR4,
            viewing_key_snip20_staked4.key
        ),
        amount1
    );

    assert_eq!(
        query_total_staked(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        amount1.checked_mul(Uint128::new(4)).unwrap()
    );
}

#[test]
fn test_auto_compounding_staking() {
    let _deps = mock_dependencies();

    let mut app = mock_app();
    let amount1 = Uint128::from(1000u128);
    let _token_address = Addr::unchecked("token_address");
    let initial_balances = vec![InitialBalance {
        address: ADDR1.to_string(),
        amount: amount1,
    }];
    let (staking_info, snip20_info) = setup_test_case(&mut app, initial_balances, None);

    let info = mock_info(ADDR1, &[]);
    let data_snip20_staked1 = create_viewing_key_snip20_staked(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
    )
    .unwrap();
    let viewing_key_snip20_staked1: CreateViewingKeyResponse =
        from_binary(&data_snip20_staked1.data.unwrap()).unwrap();

    let _env = mock_env();

    // Successful bond
    let amount = Uint128::new(100);
    stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info,
        amount,
    )
    .unwrap();
    app.update_block(next_block);
    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR1.to_string(),
            viewing_key_snip20_staked1.key.clone()
        ),
        Uint128::from(100u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(100u128)
    );
    assert_eq!(
        query_staked_value(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR1.to_string(),
            viewing_key_snip20_staked1.key.clone()
        ),
        Uint128::from(100u128)
    );
    assert_eq!(
        query_total_value(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(100u128)
    );

    // Add compounding rewards
    let msg = secret_toolkit::snip20::HandleMsg::Send {
        amount: Uint128::from(100u128),
        msg: Some(to_binary(&ReceiveMsg::Fund {}).unwrap()),
        recipient: staking_info.address.clone().to_string(),
        recipient_code_hash: Some(staking_info.code_hash.clone()),
        memo: None,
        padding: None,
    };
    let _res = app
        .borrow_mut()
        .execute_contract(Addr::unchecked(ADDR1), &snip20_info.clone(), &msg, &[])
        .unwrap();
    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR1.to_string(),
            viewing_key_snip20_staked1.key.clone()
        ),
        Uint128::from(100u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(200u128)
    );
    assert_eq!(
        query_staked_value(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR1.to_string(),
            viewing_key_snip20_staked1.key.clone()
        ),
        Uint128::from(100u128)
    );
    assert_eq!(
        query_total_value(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(200u128)
    );

    // Sucessful transfer of unbonded amount
    let msg = secret_toolkit::snip20::HandleMsg::Transfer {
        recipient: ADDR2.to_string(),
        amount: Uint128::from(100u128),
        memo: None,
        padding: None,
    };
    let _res = app
        .borrow_mut()
        .execute_contract(Addr::unchecked(ADDR1), &snip20_info.clone(), &msg, &[])
        .unwrap();

    // Addr 2 successful bond
    let info = mock_info(ADDR2, &[]);
    let data_snip20_staked2 = create_viewing_key_snip20_staked(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info.clone(),
    )
    .unwrap();
    let viewing_key_snip20_staked2: CreateViewingKeyResponse =
        from_binary(&data_snip20_staked2.data.unwrap()).unwrap();

    stake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        &snip20_info.address.clone(),
        snip20_info.code_hash.clone(),
        info,
        Uint128::new(100),
    )
    .unwrap();

    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR2,
            viewing_key_snip20_staked2.key.clone()
        ),
        Uint128::from(100u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(300u128)
    );
    assert_eq!(
        query_staked_value(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR2.to_string(),
            viewing_key_snip20_staked2.key.clone()
        ),
        Uint128::from(100u128)
    );
    assert_eq!(
        query_total_value(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(300u128)
    );

    // Can't unstake more than you have staked
    let info = mock_info(ADDR2, &[]);
    let _err = unstake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info,
        Uint128::new(300),
    )
    .unwrap_err();

    // Add compounding rewards
    let msg = secret_toolkit::snip20::HandleMsg::Send {
        amount: Uint128::from(90u128),
        msg: Some(to_binary(&ReceiveMsg::Fund {}).unwrap()),
        recipient: staking_info.address.clone().to_string(),
        recipient_code_hash: Some(staking_info.code_hash.clone()),
        memo: None,
        padding: None,
    };
    let _res = app
        .borrow_mut()
        .execute_contract(Addr::unchecked(ADDR1), &snip20_info.clone(), &msg, &[])
        .unwrap();

    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR1.to_string(),
            viewing_key_snip20_staked1.key.clone()
        ),
        Uint128::from(100u128)
    );
    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR2,
            viewing_key_snip20_staked2.key.clone()
        ),
        Uint128::from(100u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(390u128)
    );
    assert_eq!(
        query_staked_value(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR1.to_string(),
            viewing_key_snip20_staked1.key.clone()
        ),
        Uint128::from(100u128)
    );
    assert_eq!(
        query_staked_value(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR2.to_string(),
            viewing_key_snip20_staked2.key.clone()
        ),
        Uint128::from(100u128)
    );
    assert_eq!(
        query_total_value(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(390u128)
    );

    // Successful unstake
    let info = mock_info(ADDR2, &[]);
    let _res = unstake_tokens(
        &mut app,
        &staking_info.address.clone(),
        staking_info.code_hash.clone(),
        info,
        Uint128::new(25),
    )
    .unwrap();
    app.update_block(next_block);

    assert_eq!(
        query_staked_balance(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone(),
            ADDR2,
            viewing_key_snip20_staked2.key.clone()
        ),
        Uint128::from(75u128)
    );
    assert_eq!(
        query_total_staked(
            &app,
            &staking_info.address.clone(),
            staking_info.code_hash.clone()
        ),
        Uint128::from(365u128)
    );
}

// #[test]
// fn test_simple_unstaking_with_duration() {
//     let _deps = mock_dependencies();

//     let mut app = mock_app();
//     let amount1 = Uint128::from(100u128);
//     let _token_address = Addr::unchecked("token_address");
//     let initial_balances = vec![
//         InitialBalance {
//             address: ADDR1.to_string(),
//             amount: amount1,
//         },
//         InitialBalance {
//             address: ADDR2.to_string(),
//             amount: amount1,
//         },
//     ];
//     let (staking_addr, snip20_addr) =
//         setup_test_case(&mut app, initial_balances, Some(Duration::Height(1)));

//     // Bond Address 1
//     let info = mock_info(ADDR1, &[]);
//     let _env = mock_env();
//     let amount = Uint128::new(100);
//     stake_tokens(&mut app, &staking_addr, &snip20_addr, info, amount).unwrap();

//     // Bond Address 2
//     let info = mock_info(ADDR2, &[]);
//     let _env = mock_env();
//     let amount = Uint128::new(100);
//     stake_tokens(&mut app, &staking_addr, &snip20_addr, info, amount).unwrap();
//     app.update_block(next_block);
//     assert_eq!(
//         query_staked_balance(&app, &staking_addr, ADDR1.to_string()),
//         Uint128::from(100u128)
//     );
//     assert_eq!(
//         query_staked_balance(&app, &staking_addr, ADDR1.to_string()),
//         Uint128::from(100u128)
//     );

//     // Unstake Addr1
//     let info = mock_info(ADDR1, &[]);
//     let _env = mock_env();
//     let amount = Uint128::new(100);
//     unstake_tokens(&mut app, &staking_addr, info, amount).unwrap();

//     // Unstake Addr2
//     let info = mock_info(ADDR2, &[]);
//     let _env = mock_env();
//     let amount = Uint128::new(100);
//     unstake_tokens(&mut app, &staking_addr, info, amount).unwrap();

//     app.update_block(next_block);

//     assert_eq!(
//         query_staked_balance(&app, &staking_addr, ADDR1.to_string()),
//         Uint128::from(0u128)
//     );
//     assert_eq!(
//         query_staked_balance(&app, &staking_addr, ADDR2.to_string()),
//         Uint128::from(0u128)
//     );

//     // Claim
//     assert_eq!(
//         query_claims(&app, &staking_addr, ADDR1),
//         vec![Claim {
//             amount: Uint128::new(100),
//             release_at: AtHeight(12349)
//         }]
//     );
//     assert_eq!(
//         query_claims(&app, &staking_addr, ADDR2),
//         vec![Claim {
//             amount: Uint128::new(100),
//             release_at: AtHeight(12349)
//         }]
//     );

//     let info = mock_info(ADDR1, &[]);
//     claim_tokens(&mut app, &staking_addr, info).unwrap();
//     assert_eq!(get_balance(&app, &snip20_addr, ADDR1), Uint128::from(100u128));

//     let info = mock_info(ADDR2, &[]);
//     claim_tokens(&mut app, &staking_addr, info).unwrap();
//     assert_eq!(get_balance(&app, &snip20_addr, ADDR2), Uint128::from(100u128));
// }

// #[test]
// fn test_double_unstake_at_height() {
//     let mut app = App::default();

//     let (staking_addr, snip20_addr) = setup_test_case(
//         &mut app,
//         vec![InitialBalance {
//             address: "ekez".to_string(),
//             amount: Uint128::new(10),
//         }],
//         None,
//     );

//     stake_tokens(
//         &mut app,
//         &staking_addr,
//         &snip20_addr,
//         mock_info("ekez", &[]),
//         Uint128::new(10),
//     )
//     .unwrap();

//     app.update_block(next_block);

//     unstake_tokens(
//         &mut app,
//         &staking_addr,
//         mock_info("ekez", &[]),
//         Uint128::new(1),
//     )
//     .unwrap();

//     unstake_tokens(
//         &mut app,
//         &staking_addr,
//         mock_info("ekez", &[]),
//         Uint128::new(9),
//     )
//     .unwrap();

//     app.update_block(next_block);

//     // Unstaked balances are not reflected until the following
//     // block. Same behavior as staked balances. This is important
//     // because otherwise weird things could happen like:
//     //
//     // 1. I create a proposal (and am allowed to because I have a
//     //    staked balance)
//     // 2. I unstake all my tokens in the same block.
//     //
//     // Now there is some strangeness as for part of the block I had a
//     // staked balance and was allowed to take actions as if I did, and
//     // part of it I did not.
//     let balance: StakedBalanceAtHeightResponse = app
//         .wrap()
//         .query_wasm_smart(
//             staking_addr.clone(),
//             &QueryMsg::StakedBalanceAtHeight {
//                 address: "ekez".to_string(),
//                 height: Some(app.block_info().height - 1),
//             },
//         )
//         .unwrap();

//     assert_eq!(balance.balance, Uint128::new(10));

//     let balance: StakedBalanceAtHeightResponse = app
//         .wrap()
//         .query_wasm_smart(
//             staking_addr,
//             &QueryMsg::StakedBalanceAtHeight {
//                 address: "ekez".to_string(),
//                 height: Some(app.block_info().height),
//             },
//         )
//         .unwrap();

//     assert_eq!(balance.balance, Uint128::zero())
// }

// #[test]
// fn test_query_list_stakers() {
//     let mut app = App::default();

//     let (staking_addr, snip20_addr) = setup_test_case(
//         &mut app,
//         vec![
//             InitialBalance {
//                 address: "ekez1".to_string(),
//                 amount: Uint128::new(10),
//             },
//             InitialBalance {
//                 address: "ekez2".to_string(),
//                 amount: Uint128::new(20),
//             },
//             InitialBalance {
//                 address: "ekez3".to_string(),
//                 amount: Uint128::new(30),
//             },
//             InitialBalance {
//                 address: "ekez4".to_string(),
//                 amount: Uint128::new(40),
//             },
//         ],
//         None,
//     );

//     stake_tokens(
//         &mut app,
//         &staking_addr,
//         &snip20_addr,
//         mock_info("ekez1", &[]),
//         Uint128::new(10),
//     )
//     .unwrap();

//     stake_tokens(
//         &mut app,
//         &staking_addr,
//         &snip20_addr,
//         mock_info("ekez2", &[]),
//         Uint128::new(20),
//     )
//     .unwrap();

//     stake_tokens(
//         &mut app,
//         &staking_addr,
//         &snip20_addr,
//         mock_info("ekez3", &[]),
//         Uint128::new(30),
//     )
//     .unwrap();

//     stake_tokens(
//         &mut app,
//         &staking_addr,
//         &snip20_addr,
//         mock_info("ekez4", &[]),
//         Uint128::new(40),
//     )
//     .unwrap();

//     // check first 2
//     let stakers: ListStakersResponse = app
//         .wrap()
//         .query_wasm_smart(
//             staking_addr.clone(),
//             &QueryMsg::ListStakers {
//                 start_after: None,
//                 limit: Some(2),
//             },
//         )
//         .unwrap();

//     let test_res = ListStakersResponse {
//         stakers: vec![
//             StakerBalanceResponse {
//                 address: "ekez1".to_string(),
//                 balance: Uint128::new(10),
//             },
//             StakerBalanceResponse {
//                 address: "ekez2".to_string(),
//                 balance: Uint128::new(20),
//             },
//         ],
//     };

//     assert_eq!(stakers, test_res);

//     // skip first and grab 2
//     let stakers: ListStakersResponse = app
//         .wrap()
//         .query_wasm_smart(
//             staking_addr,
//             &QueryMsg::ListStakers {
//                 start_after: Some("ekez1".to_string()),
//                 limit: Some(2),
//             },
//         )
//         .unwrap();

//     let test_res = ListStakersResponse {
//         stakers: vec![
//             StakerBalanceResponse {
//                 address: "ekez2".to_string(),
//                 balance: Uint128::new(20),
//             },
//             StakerBalanceResponse {
//                 address: "ekez3".to_string(),
//                 balance: Uint128::new(30),
//             },
//         ],
//     };

//     assert_eq!(stakers, test_res)
// }

// #[test]
// fn test_ownership_transfer() {
//     let mut app = App::default();
//     let snip20_addr = instantiate_snip20(
//         &mut app,
//         vec![snip20::InitialBalance {
//             address: OWNER.to_string(),
//             amount: Uint128::from(1000u64),
//         }],
//     );
//     let staking_addr = instantiate_staking(&mut app, snip20_addr, None);

//     app.execute_contract(
//         Addr::unchecked(OWNER),
//         staking_addr.clone(),
//         &ExecuteMsg::UpdateOwnership(Action::TransferOwnership {
//             new_owner: ADDR1.to_string(),
//             expiry: None,
//         }),
//         &[],
//     )
//     .unwrap();

//     let ownership = query_owner(&app, &staking_addr);
//     assert_eq!(
//         ownership,
//         Ownership::<Addr> {
//             owner: Some(Addr::unchecked(OWNER)),
//             pending_owner: Some(Addr::unchecked(ADDR1)),
//             pending_expiry: None
//         }
//     );

//     app.execute_contract(
//         Addr::unchecked(ADDR1),
//         staking_addr.clone(),
//         &ExecuteMsg::UpdateOwnership(Action::AcceptOwnership),
//         &[],
//     )
//     .unwrap();

//     let ownership = query_owner(&app, &staking_addr);
//     assert_eq!(
//         ownership,
//         Ownership::<Addr> {
//             owner: Some(Addr::unchecked(ADDR1)),
//             pending_owner: None,
//             pending_expiry: None
//         }
//     );
// }

// #[test]
// fn test_migrate_from_v1() {
//     let mut app = App::default();
//     let snip20_addr = instantiate_snip20(
//         &mut app,
//         vec![snip20::InitialBalance {
//             address: OWNER.to_string(),
//             amount: Uint128::from(1000u64),
//         }],
//     );

//     let v1_code = app.store_code(contract_staking_v1());
//     let v2_code = app.store_code(contract_staking());

//     let staking = app
//         .instantiate_contract(
//             v1_code,
//             Addr::unchecked(OWNER),
//             &v1::msg::InstantiateMsg {
//                 owner: Some(OWNER.to_string()),
//                 manager: Some(OWNER.to_string()),
//                 token_address: snip20_addr.to_string(),
//                 unstaking_duration: None,
//             },
//             &[],
//             "staking".to_string(),
//             Some(OWNER.to_string()),
//         )
//         .unwrap();

//     app.execute(
//         Addr::unchecked(OWNER),
//         WasmMsg::Migrate {
//             contract_addr: staking.to_string(),
//             new_code_id: v2_code,
//             msg: to_binary(&MigrateMsg::FromV1 {}).unwrap(),
//         }
//         .into(),
//     )
//     .unwrap();

//     // can not migrate more than once.
//     let err: ContractError = app
//         .execute(
//             Addr::unchecked(OWNER),
//             WasmMsg::Migrate {
//                 contract_addr: staking.to_string(),
//                 new_code_id: v2_code,
//                 msg: to_binary(&MigrateMsg::FromV1 {}).unwrap(),
//             }
//             .into(),
//         )
//         .unwrap_err()
//         .downcast()
//         .unwrap();
//     assert_eq!(err, ContractError::AlreadyMigrated {});

//     // owner is moved into cw_ownable.
//     let ownership = query_owner(&app, &staking);
//     assert_eq!(
//         ownership,
//         Ownership::<Addr> {
//             owner: Some(Addr::unchecked(OWNER)),
//             pending_owner: None,
//             pending_expiry: None
//         }
//     );

//     // config is loadable and has no manager, but is otherwise
//     // unchanged.
//     let config = query_config(&app, &staking);
//     assert_eq!(
//         config,
//         Config {
//             token_address: snip20_addr,
//             unstaking_duration: None,
//         }
//     );
// }
