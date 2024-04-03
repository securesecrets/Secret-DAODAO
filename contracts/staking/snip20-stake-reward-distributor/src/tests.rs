use crate::{
    contract::reply,
    msg::{ExecuteMsg, InfoResponse, InstantiateMsg, QueryMsg},
    state::Config,
    ContractError,
};
use cosmwasm_std::{from_binary, to_binary, Addr, ContractInfo, Empty, Uint128};
use cw_ownable::{Action, Expiration, Ownership, OwnershipError};
use secret_multi_test::{next_block, App, Contract, ContractWrapper, Executor};
use shade_protocol::utils::asset::RawContract;
use snip20_reference_impl::msg::InitialBalance;

const OWNER: &str = "owner";
const OWNER2: &str = "owner2";

pub fn cw20_contract() -> Box<dyn Contract<Empty>> {
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

fn contract_query_auth() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        query_auth::contract::execute,
        query_auth::contract::instantiate,
        query_auth::contract::query,
    );
    Box::new(contract)
}

fn distributor_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_reply(reply)
    .with_migrate(crate::contract::migrate);
    Box::new(contract)
}

fn instantiate_snip20(app: &mut App, initial_balances: Vec<InitialBalance>) -> ContractInfo {
    let contract_info = app.store_code(cw20_contract());
    let msg = snip20_reference_impl::msg::InstantiateMsg {
        name: String::from("Test"),
        symbol: String::from("TEST"),
        decimals: 6,
        initial_balances: Some(initial_balances),
        admin: None,
        prng_seed: to_binary(&"prng_seed".to_string()).unwrap(),
        config: None,
        supported_denoms: None,
    };

    app.instantiate_contract(
        contract_info,
        Addr::unchecked(OWNER),
        &msg,
        &[],
        "cw20",
        None,
    )
    .unwrap()
}

fn instantiate_staking(
    app: &mut App,
    snip20_addr: Addr,
    snip20_code_hash: String,
    query_auth: RawContract,
) -> ContractInfo {
    let contract_info = app.store_code(staking_contract());
    let msg = snip20_stake::msg::InstantiateMsg {
        owner: Some(OWNER.to_string()),
        token_address: snip20_addr.to_string(),
        token_code_hash: Some(snip20_code_hash),
        unstaking_duration: None,
        query_auth,
    };
    app.instantiate_contract(
        contract_info,
        Addr::unchecked(OWNER),
        &msg,
        &[],
        "staking",
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
        Addr::unchecked(OWNER),
        &msg,
        &[],
        "query_auth",
        None,
    )
    .unwrap()
}

fn instantiate_distributor(app: &mut App, msg: InstantiateMsg) -> ContractInfo {
    let contract_info = app.store_code(distributor_contract());
    app.instantiate_contract(
        contract_info,
        Addr::unchecked(OWNER),
        &msg,
        &[],
        "distributor",
        None,
    )
    .unwrap()
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

fn get_info<T: Into<String>, C: Into<String>>(
    app: &App,
    distributor_addr: T,
    distributor_code_hash: C,
) -> InfoResponse {
    let result: InfoResponse = app
        .wrap()
        .query_wasm_smart(distributor_code_hash, distributor_addr, &QueryMsg::Info {})
        .unwrap();
    result
}

fn get_owner(app: &App, contract: &Addr, code_hash: String) -> Ownership<Addr> {
    app.wrap()
        .query_wasm_smart(code_hash, contract, &QueryMsg::Ownership {})
        .unwrap()
}

fn create_viewing_key_snip20(app: &mut App, contract_info: ContractInfo, addr: Addr) -> String {
    let msg = snip20_reference_impl::msg::ExecuteMsg::CreateViewingKey {
        entropy: "entropy".to_string(),
        padding: None,
    };
    let res = app
        .execute_contract(addr, &contract_info, &msg, &[])
        .unwrap();
    let mut viewing_key = String::new();
    let data: snip20_reference_impl::msg::ExecuteAnswer = from_binary(&res.data.unwrap()).unwrap();
    if let snip20_reference_impl::msg::ExecuteAnswer::CreateViewingKey { key } = data {
        viewing_key = key;
    };
    viewing_key
}

#[test]
fn test_instantiate() {
    let mut app = App::default();

    let snip20_info = instantiate_snip20(&mut app, vec![]);
    let query_auth_info = instantiate_query_auth(&mut app);
    let staking_info = instantiate_staking(
        &mut app,
        snip20_info.address.clone(),
        snip20_info.clone().code_hash,
        RawContract {
            address: query_auth_info.clone().address.to_string(),
            code_hash: query_auth_info.clone().code_hash,
        },
    );

    let msg = InstantiateMsg {
        owner: OWNER.to_string(),
        staking_addr: staking_info.clone().address.to_string(),
        staking_code_hash: staking_info.clone().code_hash,
        reward_rate: Uint128::new(1),
        reward_token: snip20_info.clone().address.to_string(),
        reward_token_code_hash: snip20_info.clone().code_hash,
    };

    let distributor_info = instantiate_distributor(&mut app, msg);
    let response: InfoResponse = app
        .wrap()
        .query_wasm_smart(
            distributor_info.clone().code_hash,
            distributor_info.clone().address.to_string(),
            &QueryMsg::Info {},
        )
        .unwrap();

    assert_eq!(
        response.config,
        Config {
            staking_addr: staking_info.clone().address,
            staking_code_hash: staking_info.clone().code_hash,
            reward_rate: Uint128::new(1),
            reward_token: snip20_info.clone().address,
            reward_token_code_hash: snip20_info.clone().code_hash,
        }
    );
    assert_eq!(response.last_payment_block, app.block_info().height);

    let ownership = get_owner(
        &app,
        &distributor_info.clone().address,
        distributor_info.clone().code_hash,
    );
    assert_eq!(
        ownership,
        Ownership::<Addr> {
            owner: Some(Addr::unchecked(OWNER)),
            pending_owner: None,
            pending_expiry: None
        }
    );
}

#[test]
fn test_update_config() {
    let mut app = App::default();

    let snip20_info = instantiate_snip20(&mut app, vec![]);
    let query_auth_info = instantiate_query_auth(&mut app);
    let staking_info = instantiate_staking(
        &mut app,
        snip20_info.address.clone(),
        snip20_info.clone().code_hash,
        RawContract {
            address: query_auth_info.clone().address.to_string(),
            code_hash: query_auth_info.clone().code_hash,
        },
    );

    let msg = InstantiateMsg {
        owner: OWNER.to_string(),
        staking_addr: staking_info.clone().address.to_string(),
        staking_code_hash: staking_info.clone().code_hash,
        reward_rate: Uint128::new(1),
        reward_token: snip20_info.clone().address.to_string(),
        reward_token_code_hash: snip20_info.clone().code_hash,
    };

    let distributor_info = instantiate_distributor(&mut app, msg);

    let msg = ExecuteMsg::UpdateConfig {
        staking_addr: staking_info.clone().address.to_string(),
        staking_code_hash: staking_info.clone().code_hash,
        reward_rate: Uint128::new(5),
        reward_token: snip20_info.clone().address.to_string(),
        reward_token_code_hash: snip20_info.clone().code_hash,
    };

    app.execute_contract(Addr::unchecked(OWNER), &distributor_info.clone(), &msg, &[])
        .unwrap();

    let response: InfoResponse = app
        .wrap()
        .query_wasm_smart(
            distributor_info.clone().code_hash,
            distributor_info.clone().address.to_string(),
            &QueryMsg::Info {},
        )
        .unwrap();

    assert_eq!(
        response.config,
        Config {
            staking_addr: staking_info.clone().address,
            staking_code_hash: staking_info.clone().code_hash,
            reward_rate: Uint128::new(5),
            reward_token: snip20_info.clone().address,
            reward_token_code_hash: snip20_info.clone().code_hash,
        }
    );

    let msg = ExecuteMsg::UpdateConfig {
        staking_addr: staking_info.clone().address.to_string(),
        staking_code_hash: staking_info.clone().code_hash,
        reward_rate: Uint128::new(7),
        reward_token: snip20_info.clone().address.to_string(),
        reward_token_code_hash: snip20_info.clone().code_hash,
    };

    // non-owner may not update config.
    let err: ContractError = app
        .execute_contract(Addr::unchecked("notowner"), &distributor_info, &msg, &[])
        .unwrap_err()
        .downcast()
        .unwrap();

    assert_eq!(err, ContractError::Ownership(OwnershipError::NotOwner));
}

#[test]
fn test_distribute() {
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
        snip20_info.address.clone(),
        snip20_info.clone().code_hash,
        RawContract {
            address: query_auth_info.clone().address.to_string(),
            code_hash: query_auth_info.clone().code_hash,
        },
    );

    let msg = InstantiateMsg {
        owner: OWNER.to_string(),
        staking_addr: staking_info.clone().address.to_string(),
        staking_code_hash: staking_info.clone().code_hash,
        reward_rate: Uint128::new(1),
        reward_token: snip20_info.clone().address.to_string(),
        reward_token_code_hash: snip20_info.clone().code_hash,
    };

    let distributor_contract_info = instantiate_distributor(&mut app, msg);

    let msg = snip20_reference_impl::msg::ExecuteMsg::Transfer {
        recipient: distributor_contract_info.clone().address.to_string(),
        amount: Uint128::from(1000u128),
        memo: None,
        decoys: None,
        entropy: None,
        padding: None,
    };
    app.execute_contract(Addr::unchecked(OWNER), &snip20_info.clone(), &msg, &[])
        .unwrap();

    app.update_block(|block| block.height += 10);
    app.execute_contract(
        Addr::unchecked(OWNER),
        &distributor_contract_info.clone(),
        &ExecuteMsg::Distribute {},
        &[],
    )
    .unwrap();

    let viewing_key_staking_snip20 =
        create_viewing_key_snip20(&mut app, snip20_info.clone(), staking_info.clone().address);
    let staking_balance = get_balance_snip20(
        &app,
        snip20_info.clone().address.to_string(),
        snip20_info.clone().code_hash,
        staking_info.clone().address.to_string(),
        viewing_key_staking_snip20.clone(),
    );
    assert_eq!(staking_balance, Uint128::new(10));

    let distributor_info = get_info(
        &app,
        distributor_contract_info.clone().address.to_string(),
        distributor_contract_info.clone().code_hash,
    );
    assert_eq!(distributor_info.balance, Uint128::new(990));
    assert_eq!(distributor_info.last_payment_block, app.block_info().height);

    app.update_block(|block| block.height += 500);
    app.execute_contract(
        Addr::unchecked(OWNER),
        &distributor_contract_info.clone(),
        &ExecuteMsg::Distribute {},
        &[],
    )
    .unwrap();

    let staking_balance = get_balance_snip20(
        &app,
        snip20_info.clone().address.to_string(),
        snip20_info.clone().code_hash,
        staking_info.clone().address.to_string(),
        viewing_key_staking_snip20.clone(),
    );
    assert_eq!(staking_balance, Uint128::new(510));

    let distributor_info = get_info(
        &app,
        distributor_contract_info.clone().address.to_string(),
        distributor_contract_info.clone().code_hash,
    );
    assert_eq!(distributor_info.balance, Uint128::new(490));
    assert_eq!(distributor_info.last_payment_block, app.block_info().height);

    app.update_block(|block| block.height += 1000);
    app.execute_contract(
        Addr::unchecked(OWNER),
        &distributor_contract_info.clone(),
        &ExecuteMsg::Distribute {},
        &[],
    )
    .unwrap();

    let staking_balance = get_balance_snip20(
        &app,
        snip20_info.clone().address.to_string(),
        snip20_info.clone().code_hash,
        staking_info.clone().address.to_string(),
        viewing_key_staking_snip20.clone(),
    );
    assert_eq!(staking_balance, Uint128::new(1000));

    let distributor_info = get_info(
        &app,
        distributor_contract_info.clone().address.to_string(),
        distributor_contract_info.clone().code_hash,
    );
    assert_eq!(distributor_info.balance, Uint128::new(0));
    assert_eq!(distributor_info.last_payment_block, app.block_info().height);
    let last_payment_block = distributor_info.last_payment_block;

    // Pays out nothing
    app.update_block(|block| block.height += 1100);
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(OWNER),
            &distributor_contract_info.clone(),
            &ExecuteMsg::Distribute {},
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    assert!(matches!(err, ContractError::ZeroRewards {}));

    let staking_balance = get_balance_snip20(
        &app,
        snip20_info.clone().address.to_string(),
        snip20_info.clone().code_hash,
        staking_info.clone().address.to_string(),
        viewing_key_staking_snip20.clone(),
    );
    assert_eq!(staking_balance, Uint128::new(1000));

    let distributor_info = get_info(
        &app,
        distributor_contract_info.clone().address.to_string(),
        distributor_contract_info.clone().code_hash,
    );
    assert_eq!(distributor_info.balance, Uint128::new(0));
    assert_eq!(distributor_info.last_payment_block, last_payment_block);

    // go to a block before the last payment
    app.update_block(|block| block.height -= 2000);
    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(OWNER),
            &distributor_contract_info.clone(),
            &ExecuteMsg::Distribute {},
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert!(matches!(err, ContractError::RewardsDistributedForBlock {}));
}

#[test]
fn test_instantiate_invalid_addrs() {
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
        snip20_info.address.clone(),
        snip20_info.clone().code_hash,
        RawContract {
            address: query_auth_info.clone().address.to_string(),
            code_hash: query_auth_info.clone().code_hash,
        },
    );

    let msg = InstantiateMsg {
        owner: OWNER.to_string(),
        staking_addr: staking_info.clone().address.to_string(),
        staking_code_hash: staking_info.clone().code_hash,
        reward_rate: Uint128::new(1),
        reward_token: "invalid_snip20".to_string(),
        reward_token_code_hash: "invalid_snip20_code_hash".to_string(),
    };

    let contract_info = app.store_code(distributor_contract());
    let err: ContractError = app
        .instantiate_contract(
            contract_info.clone(),
            Addr::unchecked(OWNER),
            &msg,
            &[],
            "distributor",
            None,
        )
        .unwrap_err()
        .downcast()
        .unwrap();

    assert_eq!(err, ContractError::InvalidSnip20 {});

    let msg = InstantiateMsg {
        owner: OWNER.to_string(),
        staking_addr: "invalid_staking".to_string(),
        staking_code_hash: "invalid_staking_code_hash".to_string(),
        reward_rate: Uint128::new(1),
        reward_token: snip20_info.clone().address.to_string(),
        reward_token_code_hash: snip20_info.clone().code_hash,
    };
    let err: ContractError = app
        .instantiate_contract(
            contract_info,
            Addr::unchecked(OWNER),
            &msg,
            &[],
            "distributor",
            None,
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::InvalidStakingContract {});
}

#[test]
fn test_update_config_invalid_addrs() {
    let mut app = App::default();

    let snip20_info = instantiate_snip20(&mut app, vec![]);
    let query_auth_info = instantiate_query_auth(&mut app);
    let staking_info = instantiate_staking(
        &mut app,
        snip20_info.address.clone(),
        snip20_info.clone().code_hash,
        RawContract {
            address: query_auth_info.clone().address.to_string(),
            code_hash: query_auth_info.clone().code_hash,
        },
    );

    let msg = InstantiateMsg {
        owner: OWNER.to_string(),
        staking_addr: staking_info.clone().address.to_string(),
        staking_code_hash: staking_info.clone().code_hash,
        reward_rate: Uint128::new(1),
        reward_token: snip20_info.clone().address.to_string(),
        reward_token_code_hash: snip20_info.clone().code_hash,
    };

    let distributor_contract_info = instantiate_distributor(&mut app, msg);

    let msg = ExecuteMsg::UpdateConfig {
        staking_addr: staking_info.clone().address.to_string(),
        staking_code_hash: staking_info.clone().code_hash,
        reward_rate: Uint128::new(5),
        reward_token: "invalid_snip20".to_string(),
        reward_token_code_hash: "invalid_snip20_code_hash".to_string(),
    };

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(OWNER),
            &distributor_contract_info.clone(),
            &msg,
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::InvalidSnip20 {});

    let msg = ExecuteMsg::UpdateConfig {
        staking_addr: "invalid_staking".to_string(),
        staking_code_hash: "invalid_staking_code_hash".to_string(),
        reward_rate: Uint128::new(5),
        reward_token: snip20_info.clone().address.to_string(),
        reward_token_code_hash: snip20_info.clone().code_hash,
    };

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(OWNER),
            &distributor_contract_info,
            &msg,
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::InvalidStakingContract {});
}

#[test]
fn test_withdraw() {
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
        snip20_info.address.clone(),
        snip20_info.clone().code_hash,
        RawContract {
            address: query_auth_info.clone().address.to_string(),
            code_hash: query_auth_info.clone().code_hash,
        },
    );

    let msg = InstantiateMsg {
        owner: OWNER.to_string(),
        staking_addr: staking_info.clone().address.to_string(),
        staking_code_hash: staking_info.clone().code_hash,
        reward_rate: Uint128::new(1),
        reward_token: snip20_info.clone().address.to_string(),
        reward_token_code_hash: snip20_info.clone().code_hash,
    };
    let distributor_contract_info = instantiate_distributor(&mut app, msg);

    let msg = snip20_reference_impl::msg::ExecuteMsg::Transfer {
        recipient: distributor_contract_info.clone().address.to_string(),
        amount: Uint128::from(1000u128),
        memo: None,
        decoys: None,
        entropy: None,
        padding: None,
    };
    app.execute_contract(Addr::unchecked(OWNER), &snip20_info.clone(), &msg, &[])
        .unwrap();

    app.update_block(|block| block.height += 10);
    app.execute_contract(
        Addr::unchecked(OWNER),
        &distributor_contract_info.clone(),
        &ExecuteMsg::Distribute {},
        &[],
    )
    .unwrap();

    let viewing_key_staking_snip20 =
        create_viewing_key_snip20(&mut app, snip20_info.clone(), staking_info.clone().address);
    let viewing_key_owner_snip20 =
        create_viewing_key_snip20(&mut app, snip20_info.clone(), Addr::unchecked(OWNER));

    let staking_balance = get_balance_snip20(
        &app,
        snip20_info.clone().address.to_string(),
        snip20_info.clone().code_hash,
        staking_info.clone().address.to_string(),
        viewing_key_staking_snip20.clone(),
    );
    assert_eq!(staking_balance, Uint128::new(10));

    let distributor_info = get_info(
        &app,
        distributor_contract_info.clone().address.to_string(),
        distributor_contract_info.clone().code_hash,
    );
    assert_eq!(distributor_info.balance, Uint128::new(990));
    assert_eq!(distributor_info.last_payment_block, app.block_info().height);

    // Unauthorized user cannot withdraw funds
    let err = app
        .execute_contract(
            Addr::unchecked("notowner"),
            &distributor_contract_info.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
        .unwrap_err();

    assert_eq!(
        ContractError::Ownership(OwnershipError::NotOwner),
        err.downcast().unwrap()
    );

    // Withdraw funds
    app.execute_contract(
        Addr::unchecked(OWNER),
        &distributor_contract_info,
        &ExecuteMsg::Withdraw {},
        &[],
    )
    .unwrap();

    let owner_balance = get_balance_snip20(
        &app,
        snip20_info.address.to_string(),
        snip20_info.code_hash,
        Addr::unchecked(OWNER),
        viewing_key_owner_snip20,
    );
    assert_eq!(owner_balance, Uint128::new(990));
}

#[test]
fn test_dao_deploy() {
    // DAOs will deploy this contract with following steps
    // Contract is instantiated by any address with 0 reward rate
    // Dao updates reward rate and funds in same transaction
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
        snip20_info.address.clone(),
        snip20_info.clone().code_hash,
        RawContract {
            address: query_auth_info.clone().address.to_string(),
            code_hash: query_auth_info.clone().code_hash,
        },
    );

    let msg = InstantiateMsg {
        owner: OWNER.to_string(),
        staking_addr: staking_info.clone().address.to_string(),
        staking_code_hash: staking_info.clone().code_hash,
        reward_rate: Uint128::new(1),
        reward_token: snip20_info.clone().address.to_string(),
        reward_token_code_hash: snip20_info.clone().code_hash,
    };
    let distributor_contract_info = instantiate_distributor(&mut app, msg);

    let msg = ExecuteMsg::UpdateConfig {
        staking_addr: staking_info.clone().address.to_string(),
        staking_code_hash: staking_info.clone().code_hash,
        reward_rate: Uint128::new(1),
        reward_token: snip20_info.clone().address.to_string(),
        reward_token_code_hash: staking_info.clone().code_hash,
    };
    app.execute_contract(
        Addr::unchecked(OWNER),
        &distributor_contract_info.clone(),
        &msg,
        &[],
    )
    .unwrap();

    let msg = snip20_reference_impl::msg::ExecuteMsg::Transfer {
        recipient: distributor_contract_info.clone().address.to_string(),
        amount: Uint128::from(1000u128),
        memo: None,
        decoys: None,
        entropy: None,
        padding: None,
    };
    app.execute_contract(Addr::unchecked(OWNER), &snip20_info.clone(), &msg, &[])
        .unwrap();

    app.update_block(|block| block.height += 10);
    app.execute_contract(
        Addr::unchecked(OWNER),
        &distributor_contract_info.clone(),
        &ExecuteMsg::Distribute {},
        &[],
    )
    .unwrap();

    let viewing_key_staking_snip20 =
        create_viewing_key_snip20(&mut app, snip20_info.clone(), staking_info.clone().address);

    let staking_balance = get_balance_snip20(
        &app,
        snip20_info.clone().address.to_string(),
        snip20_info.clone().code_hash,
        staking_info.clone().address.to_string(),
        viewing_key_staking_snip20.clone(),
    );
    assert_eq!(staking_balance, Uint128::new(10));

    let distributor_info = get_info(
        &app,
        distributor_contract_info.clone().address.to_string(),
        distributor_contract_info.clone().code_hash,
    );
    assert_eq!(distributor_info.balance, Uint128::new(990));
    assert_eq!(distributor_info.last_payment_block, app.block_info().height);
}

#[test]
fn test_ownership() {
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
        snip20_info.address.clone(),
        snip20_info.clone().code_hash,
        RawContract {
            address: query_auth_info.clone().address.to_string(),
            code_hash: query_auth_info.clone().code_hash,
        },
    );

    let msg = InstantiateMsg {
        owner: OWNER.to_string(),
        staking_addr: staking_info.clone().address.to_string(),
        staking_code_hash: staking_info.clone().code_hash,
        reward_rate: Uint128::new(1),
        reward_token: snip20_info.clone().address.to_string(),
        reward_token_code_hash: snip20_info.clone().code_hash,
    };
    let distributor_contract_info = instantiate_distributor(&mut app, msg);

    app.execute_contract(
        Addr::unchecked(OWNER),
        &distributor_contract_info.clone(),
        &ExecuteMsg::UpdateOwnership(Action::TransferOwnership {
            new_owner: OWNER2.to_string(),
            expiry: None,
        }),
        &[],
    )
    .unwrap();

    let ownership = get_owner(
        &app,
        &distributor_contract_info.clone().address,
        distributor_contract_info.clone().code_hash,
    );
    assert_eq!(
        ownership,
        Ownership::<Addr> {
            owner: Some(Addr::unchecked(OWNER)),
            pending_owner: Some(Addr::unchecked(OWNER2)),
            pending_expiry: None
        }
    );

    app.execute_contract(
        Addr::unchecked(OWNER2),
        &distributor_contract_info.clone(),
        &ExecuteMsg::UpdateOwnership(Action::AcceptOwnership),
        &[],
    )
    .unwrap();

    let ownership = get_owner(
        &app,
        &distributor_contract_info.clone().address,
        distributor_contract_info.clone().code_hash,
    );
    assert_eq!(
        ownership,
        Ownership::<Addr> {
            owner: Some(Addr::unchecked(OWNER2)),
            pending_owner: None,
            pending_expiry: None
        }
    );
}

#[test]
fn test_ownership_expiry() {
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
        snip20_info.address.clone(),
        snip20_info.clone().code_hash,
        RawContract {
            address: query_auth_info.clone().address.to_string(),
            code_hash: query_auth_info.clone().code_hash,
        },
    );

    let msg = InstantiateMsg {
        owner: OWNER.to_string(),
        staking_addr: staking_info.clone().address.to_string(),
        staking_code_hash: staking_info.clone().code_hash,
        reward_rate: Uint128::new(1),
        reward_token: snip20_info.clone().address.to_string(),
        reward_token_code_hash: snip20_info.clone().code_hash,
    };
    let distributor_contract_info = instantiate_distributor(&mut app, msg);

    app.execute_contract(
        Addr::unchecked(OWNER),
        &distributor_contract_info.clone(),
        &ExecuteMsg::UpdateOwnership(Action::TransferOwnership {
            new_owner: OWNER2.to_string(),
            expiry: Some(Expiration::AtHeight(app.block_info().height + 1)),
        }),
        &[],
    )
    .unwrap();

    let ownership = get_owner(
        &app,
        &distributor_contract_info.clone().address,
        distributor_contract_info.clone().code_hash,
    );
    assert_eq!(
        ownership,
        Ownership::<Addr> {
            owner: Some(Addr::unchecked(OWNER)),
            pending_owner: Some(Addr::unchecked(OWNER2)),
            pending_expiry: Some(Expiration::AtHeight(app.block_info().height + 1)),
        }
    );

    app.update_block(next_block);

    let err: ContractError = app
        .execute_contract(
            Addr::unchecked(OWNER2),
            &distributor_contract_info,
            &ExecuteMsg::UpdateOwnership(Action::AcceptOwnership),
            &[],
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(
        err,
        ContractError::Ownership(OwnershipError::TransferExpired)
    )
}
