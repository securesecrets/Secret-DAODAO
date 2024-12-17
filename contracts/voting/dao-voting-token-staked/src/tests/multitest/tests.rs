use crate::contract::{migrate, CONTRACT_NAME, CONTRACT_VERSION};
use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, TokenInfo};
use crate::state::Config;
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{
    coins, from_binary, to_binary, Addr, Coin, ContractInfo, Decimal, Empty, MessageInfo, Uint128,
};
use dao_interface::state::AnyContractInfo;
use dao_interface::voting::{
    DenomResponse, InfoResponse, IsActiveResponse, TotalPowerAtHeightResponse,
    VotingPowerAtHeightResponse,
};
use dao_voting::threshold::ActiveThreshold;
use secret_cw_controllers::ClaimsResponse;
use secret_multi_test::{
    next_block, App, AppResponse, BankSudo, Contract, ContractInstantiationInfo, ContractWrapper,
    Executor, SudoMsg,
};
use secret_utils::Duration;
use shade_protocol::basic_staking::Auth;
use shade_protocol::utils::asset::RawContract;

const DAO_ADDR: &str = "dao";
const ADDR1: &str = "addr1";
const ADDR2: &str = "addr2";
const DENOM: &str = "uscrt";
const INVALID_DENOM: &str = "uinvalid";
const ODD_DENOM: &str = "uodd";

// fn hook_counter_contract() -> Box<dyn Contract<Empty>> {
//     let contract = ContractWrapper::new(
//         dao_proposal_hook_counter::contract::execute,
//         dao_proposal_hook_counter::contract::instantiate,
//         dao_proposal_hook_counter::contract::query,
//     );
//     Box::new(contract)
// }

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

fn staking_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_migrate(crate::contract::migrate);
    Box::new(contract)
}

fn query_auth_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        query_auth::contract::execute,
        query_auth::contract::instantiate,
        query_auth::contract::query,
    );
    Box::new(contract)
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
        Addr::unchecked(DAO_ADDR),
        &msg,
        &[],
        "query_auth",
        None,
    )
    .unwrap()
}

fn mock_app() -> App {
    let mut app = App::default();
    app.sudo(SudoMsg::Bank(BankSudo::Mint {
        to_address: DAO_ADDR.to_string(),
        amount: vec![
            Coin {
                denom: DENOM.to_string(),
                amount: Uint128::new(10000),
            },
            Coin {
                denom: INVALID_DENOM.to_string(),
                amount: Uint128::new(10000),
            },
        ],
    }))
    .unwrap();
    app.sudo(SudoMsg::Bank(BankSudo::Mint {
        to_address: ADDR1.to_string(),
        amount: vec![
            Coin {
                denom: DENOM.to_string(),
                amount: Uint128::new(10000),
            },
            Coin {
                denom: INVALID_DENOM.to_string(),
                amount: Uint128::new(10000),
            },
            Coin {
                denom: ODD_DENOM.to_string(),
                amount: Uint128::new(5),
            },
        ],
    }))
    .unwrap();
    app.sudo(SudoMsg::Bank(BankSudo::Mint {
        to_address: ADDR2.to_string(),
        amount: vec![
            Coin {
                denom: DENOM.to_string(),
                amount: Uint128::new(10000),
            },
            Coin {
                denom: INVALID_DENOM.to_string(),
                amount: Uint128::new(10000),
            },
        ],
    }))
    .unwrap();
    app
}

fn instantiate_staking(
    app: &mut App,
    staking_info: ContractInstantiationInfo,
    msg: InstantiateMsg,
) -> ContractInfo {
    app.instantiate_contract(
        staking_info,
        Addr::unchecked(DAO_ADDR),
        &msg,
        &[],
        "Staking",
        None,
    )
    .unwrap()
}

fn stake_tokens(
    app: &mut App,
    staking_info: ContractInfo,
    sender: &str,
    amount: u128,
    auth: Auth,
    denom: &str,
) -> anyhow::Result<AppResponse> {
    app.execute_contract(
        Addr::unchecked(sender),
        &staking_info,
        &ExecuteMsg::Stake { auth },
        &coins(amount, denom),
    )
}

fn unstake_tokens(
    app: &mut App,
    staking_info: ContractInfo,
    sender: &str,
    amount: u128,
    auth: Auth,
) -> anyhow::Result<AppResponse> {
    app.execute_contract(
        Addr::unchecked(sender),
        &staking_info,
        &ExecuteMsg::Unstake {
            auth,
            amount: Uint128::new(amount),
        },
        &[],
    )
}

fn claim(app: &mut App, staking_info: ContractInfo, sender: &str) -> anyhow::Result<AppResponse> {
    app.execute_contract(
        Addr::unchecked(sender),
        &staking_info,
        &ExecuteMsg::Claim {},
        &[],
    )
}

fn update_config(
    app: &mut App,
    staking_info: ContractInfo,
    sender: &str,
    duration: Option<Duration>,
) -> anyhow::Result<AppResponse> {
    app.execute_contract(
        Addr::unchecked(sender),
        &staking_info,
        &ExecuteMsg::UpdateConfig { duration },
        &[],
    )
}

fn get_voting_power_at_height(
    app: &mut App,
    staking_info: ContractInfo,
    auth: Auth,
    height: Option<u64>,
) -> VotingPowerAtHeightResponse {
    app.wrap()
        .query_wasm_smart(
            staking_info.code_hash,
            staking_info.address.to_string(),
            &QueryMsg::VotingPowerAtHeight { auth, height },
        )
        .unwrap()
}

fn get_total_power_at_height(
    app: &mut App,
    staking_info: ContractInfo,
    height: Option<u64>,
) -> TotalPowerAtHeightResponse {
    app.wrap()
        .query_wasm_smart(
            staking_info.code_hash,
            staking_info.address.to_string(),
            &QueryMsg::TotalPowerAtHeight { height },
        )
        .unwrap()
}

fn get_config(app: &mut App, staking_info: ContractInfo) -> Config {
    app.wrap()
        .query_wasm_smart(
            staking_info.code_hash,
            staking_info.address.to_string(),
            &QueryMsg::GetConfig {},
        )
        .unwrap()
}

fn get_claims(app: &mut App, staking_info: ContractInfo, auth: Auth) -> ClaimsResponse {
    app.wrap()
        .query_wasm_smart(
            staking_info.code_hash,
            staking_info.address.to_string(),
            &QueryMsg::Claims { auth },
        )
        .unwrap()
}

fn get_balance(app: &mut App, address: &str, denom: &str) -> Uint128 {
    app.wrap().query_balance(address, denom).unwrap().amount
}

#[test]
fn test_instantiate_existing() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    // Populated fields
    instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    // Non populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info,
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: None,
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    let denom: DenomResponse = app
        .wrap()
        .query_wasm_smart(
            staking_info.code_hash,
            staking_info.address.to_string(),
            &QueryMsg::Denom {},
        )
        .unwrap();
    assert_eq!(
        denom,
        DenomResponse {
            denom: DENOM.to_string()
        }
    );
}

#[test]
#[should_panic(expected = "Invalid unstaking duration, unstaking duration cannot be 0")]
fn test_instantiate_invalid_unstaking_duration_height() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    // Populated fields
    instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(0)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );
}

#[test]
#[should_panic(expected = "Invalid unstaking duration, unstaking duration cannot be 0")]
fn test_instantiate_invalid_unstaking_duration_time() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    // Populated fields
    instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Time(0)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );
}

#[test]
#[should_panic(expected = "Must send reserve token 'uscrt'")]
fn test_stake_invalid_denom() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let info = mock_info(ADDR1, &[]);
    let viewing_key = create_viewing_key(&mut app, query_auth.clone(), info.clone());
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    // Try and stake an invalid denom
    stake_tokens(
        &mut app,
        staking_info,
        ADDR1,
        100,
        Auth::ViewingKey {
            key: viewing_key,
            address: info.sender.to_string(),
        },
        INVALID_DENOM,
    )
    .unwrap();
}

#[test]
fn test_stake_valid_denom() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let info = mock_info(ADDR1, &[]);
    let viewing_key = create_viewing_key(&mut app, query_auth.clone(), info.clone());
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );
    // Try and stake an valid denom
    stake_tokens(
        &mut app,
        staking_info,
        ADDR1,
        100,
        Auth::ViewingKey {
            key: viewing_key,
            address: info.sender.to_string(),
        },
        DENOM,
    )
    .unwrap();
    app.update_block(next_block);
}

#[test]
#[should_panic(expected = "Can only unstake less than or equal to the amount you have staked")]
fn test_unstake_none_staked() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let info = mock_info(ADDR1, &[]);
    let viewing_key = create_viewing_key(&mut app, query_auth.clone(), info.clone());
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    unstake_tokens(
        &mut app,
        staking_info,
        ADDR1,
        100,
        Auth::ViewingKey {
            key: viewing_key,
            address: info.sender.to_string(),
        },
    )
    .unwrap();
}

#[test]
#[should_panic(expected = "Amount being unstaked must be non-zero")]
fn test_unstake_zero_tokens() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let info = mock_info(ADDR1, &[]);
    let viewing_key = create_viewing_key(&mut app, query_auth.clone(), info.clone());
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    unstake_tokens(
        &mut app,
        staking_info,
        ADDR1,
        0,
        Auth::ViewingKey {
            key: viewing_key,
            address: info.sender.to_string(),
        },
    )
    .unwrap();
}

#[test]
#[should_panic(expected = "Can only unstake less than or equal to the amount you have staked")]
fn test_unstake_invalid_balance() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let info = mock_info(ADDR1, &[]);
    let viewing_key = create_viewing_key(&mut app, query_auth.clone(), info.clone());
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    // Stake some tokens
    stake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        100,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: info.sender.to_string(),
        },
        DENOM,
    )
    .unwrap();
    app.update_block(next_block);

    // Try and unstake too many
    unstake_tokens(
        &mut app,
        staking_info,
        ADDR1,
        200,
        Auth::ViewingKey {
            key: viewing_key,
            address: info.sender.to_string(),
        },
    )
    .unwrap();
}

#[test]
fn test_unstake() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let info = mock_info(ADDR1, &[]);
    let viewing_key = create_viewing_key(&mut app, query_auth.clone(), info.clone());
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    // Stake some tokens
    stake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        100,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: info.sender.to_string(),
        },
        DENOM,
    )
    .unwrap();
    app.update_block(next_block);

    // Unstake some
    unstake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        75,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: info.sender.to_string(),
        },
    )
    .unwrap();

    // Query claims
    let claims = get_claims(
        &mut app,
        staking_info.clone(),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: ADDR1.to_string(),
        },
    );
    assert_eq!(claims.claims.len(), 1);
    app.update_block(next_block);

    // Unstake the rest
    unstake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        25,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: info.sender.to_string(),
        },
    )
    .unwrap();

    // Query claims
    let claims = get_claims(
        &mut app,
        staking_info,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: ADDR1.to_string(),
        },
    );
    assert_eq!(claims.claims.len(), 2);
}

#[test]
fn test_unstake_no_unstaking_duration() {
    let mut app = mock_app();
    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let info = mock_info(ADDR1, &[]);
    let viewing_key = create_viewing_key(&mut app, query_auth.clone(), info.clone());
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: None,
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    // Stake some tokens
    stake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        100,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: info.sender.to_string(),
        },
        DENOM,
    )
    .unwrap();
    app.update_block(next_block);

    // Unstake some tokens
    unstake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        75,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: info.sender.to_string(),
        },
    )
    .unwrap();

    app.update_block(next_block);

    let balance = get_balance(&mut app, ADDR1, DENOM);
    // 10000 (initial bal) - 100 (staked) + 75 (unstaked) = 9975
    assert_eq!(balance, Uint128::new(9975));

    // Unstake the rest
    unstake_tokens(
        &mut app,
        staking_info,
        ADDR1,
        25,
        Auth::ViewingKey {
            key: viewing_key,
            address: info.sender.to_string(),
        },
    )
    .unwrap();

    let balance = get_balance(&mut app, ADDR1, DENOM);
    // 10000 (initial bal) - 100 (staked) + 75 (unstaked 1) + 25 (unstaked 2) = 10000
    assert_eq!(balance, Uint128::new(10000))
}

#[test]
#[should_panic(expected = "Nothing to claim")]
fn test_claim_no_claims() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    claim(&mut app, staking_info, ADDR1).unwrap();
}

#[test]
#[should_panic(expected = "Nothing to claim")]
fn test_claim_claim_not_reached() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let info = mock_info(ADDR1, &[]);
    let viewing_key = create_viewing_key(&mut app, query_auth.clone(), info.clone());
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    // Stake some tokens
    stake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        100,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: info.sender.to_string(),
        },
        DENOM,
    )
    .unwrap();
    app.update_block(next_block);

    // Unstake them to create the claims
    unstake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        100,
        Auth::ViewingKey {
            key: viewing_key,
            address: info.sender.to_string(),
        },
    )
    .unwrap();
    app.update_block(next_block);

    // We have a claim but it isnt reached yet so this will still fail
    claim(&mut app, staking_info, ADDR1).unwrap();
}

#[test]
fn test_claim() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let info = mock_info(ADDR1, &[]);
    let viewing_key = create_viewing_key(&mut app, query_auth.clone(), info.clone());
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    // Stake some tokens
    stake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        100,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: info.sender.to_string(),
        },
        DENOM,
    )
    .unwrap();
    app.update_block(next_block);

    // Unstake some to create the claims
    unstake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        75,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: info.sender.to_string(),
        },
    )
    .unwrap();
    app.update_block(|b| {
        b.height += 5;
        b.time = b.time.plus_seconds(25);
    });

    // Claim
    claim(&mut app, staking_info.clone(), ADDR1).unwrap();

    // Query balance
    let balance = get_balance(&mut app, ADDR1, DENOM);
    // 10000 (initial bal) - 100 (staked) + 75 (unstaked) = 9975
    assert_eq!(balance, Uint128::new(9975));

    // Unstake the rest
    unstake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        25,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: info.sender.to_string(),
        },
    )
    .unwrap();
    app.update_block(|b| {
        b.height += 10;
        b.time = b.time.plus_seconds(50);
    });

    // Claim
    claim(&mut app, staking_info, ADDR1).unwrap();

    // Query balance
    let balance = get_balance(&mut app, ADDR1, DENOM);
    // 10000 (initial bal) - 100 (staked) + 75 (unstaked 1) + 25 (unstaked 2) = 10000
    assert_eq!(balance, Uint128::new(10000));
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_update_config_invalid_sender() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    // From ADDR2, so not owner or manager
    update_config(&mut app, staking_info, ADDR2, Some(Duration::Height(10))).unwrap();
}

#[test]
fn test_update_config_as_owner() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    // Swap owner and manager, change duration
    update_config(
        &mut app,
        staking_info.clone(),
        DAO_ADDR,
        Some(Duration::Height(10)),
    )
    .unwrap();

    let config = get_config(&mut app, staking_info);
    assert_eq!(
        Config {
            unstaking_duration: Some(Duration::Height(10)),
            query_auth: query_auth.into()
        },
        config
    );
}

#[test]
#[should_panic(expected = "Invalid unstaking duration, unstaking duration cannot be 0")]
fn test_update_config_invalid_duration() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    // Change duration and manager as manager cannot change owner
    update_config(&mut app, staking_info, DAO_ADDR, Some(Duration::Height(0))).unwrap();
}

#[test]
fn test_query_dao() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    let msg = QueryMsg::Dao {};
    let dao: AnyContractInfo = app
        .wrap()
        .query_wasm_smart(
            staking_info.code_hash,
            staking_info.address.to_string(),
            &msg,
        )
        .unwrap();
    assert_eq!(
        dao,
        AnyContractInfo {
            addr: Addr::unchecked(DAO_ADDR),
            code_hash: "".to_string()
        }
    );
}

#[test]
fn test_query_info() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    let msg = QueryMsg::Info {};
    let resp: InfoResponse = app
        .wrap()
        .query_wasm_smart(
            staking_info.code_hash,
            staking_info.address.to_string(),
            &msg,
        )
        .unwrap();
    assert_eq!(resp.info.contract, "crates.io:dao-voting-token-staked");
}

#[test]
fn test_query_claims() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let info = mock_info(ADDR1, &[]);
    let viewing_key = create_viewing_key(&mut app, query_auth.clone(), info.clone());
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    let claims = get_claims(
        &mut app,
        staking_info.clone(),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: ADDR1.to_string(),
        },
    );
    assert_eq!(claims.claims.len(), 0);

    // Stake some tokens
    stake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        100,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: info.sender.to_string(),
        },
        DENOM,
    )
    .unwrap();
    app.update_block(next_block);

    // Unstake some tokens
    unstake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        25,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: info.sender.to_string(),
        },
    )
    .unwrap();
    app.update_block(next_block);

    let claims = get_claims(
        &mut app,
        staking_info.clone(),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: ADDR1.to_string(),
        },
    );
    assert_eq!(claims.claims.len(), 1);

    unstake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        25,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: info.sender.to_string(),
        },
    )
    .unwrap();
    app.update_block(next_block);

    let claims = get_claims(
        &mut app,
        staking_info,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: ADDR1.to_string(),
        },
    );
    assert_eq!(claims.claims.len(), 2);
}

#[test]
fn test_query_get_config() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    let config = get_config(&mut app, staking_info);
    assert_eq!(
        config,
        Config {
            unstaking_duration: Some(Duration::Height(5)),
            query_auth: query_auth.into()
        }
    )
}

#[test]
fn test_voting_power_queries() {
    let mut app = mock_app();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);

    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    let viewing_key_addr1 = create_viewing_key(&mut app, query_auth.clone(), mock_info(ADDR1, &[]));

    let viewing_key_addr2 = create_viewing_key(&mut app, query_auth, mock_info(ADDR2, &[]));

    // Total power is 0
    let resp = get_total_power_at_height(&mut app, staking_info.clone(), None);
    assert!(resp.power.is_zero());

    // ADDR1 has no power, none staked
    let resp = get_voting_power_at_height(
        &mut app,
        staking_info.clone(),
        Auth::ViewingKey {
            key: viewing_key_addr1.clone(),
            address: ADDR1.to_string(),
        },
        None,
    );
    assert!(resp.power.is_zero());

    // ADDR1 stakes
    stake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        100,
        Auth::ViewingKey {
            key: viewing_key_addr1.clone(),
            address: ADDR1.to_string(),
        },
        DENOM,
    )
    .unwrap();
    app.update_block(next_block);

    // Total power is 100
    let resp = get_total_power_at_height(&mut app, staking_info.clone(), None);
    assert_eq!(resp.power, Uint128::new(100));

    // ADDR1 has 100 power
    let resp = get_voting_power_at_height(
        &mut app,
        staking_info.clone(),
        Auth::ViewingKey {
            key: viewing_key_addr1.clone(),
            address: ADDR1.to_string(),
        },
        None,
    );
    assert_eq!(resp.power, Uint128::new(100));

    // ADDR2 still has 0 power
    let resp = get_voting_power_at_height(
        &mut app,
        staking_info.clone(),
        Auth::ViewingKey {
            key: viewing_key_addr2.clone(),
            address: ADDR2.to_string(),
        },
        None,
    );
    assert!(resp.power.is_zero());

    // ADDR2 stakes
    stake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR2,
        50,
        Auth::ViewingKey {
            key: viewing_key_addr2.clone(),
            address: ADDR2.to_string(),
        },
        DENOM,
    )
    .unwrap();
    app.update_block(next_block);
    let prev_height = app.block_info().height - 2;

    // Query the previous height, total 100, ADDR1 100, ADDR2 0
    // Total power is 100
    let resp = get_total_power_at_height(&mut app, staking_info.clone(), Some(prev_height));
    assert_eq!(resp.power, Uint128::new(100));

    // ADDR1 has 100 power
    let resp = get_voting_power_at_height(
        &mut app,
        staking_info.clone(),
        Auth::ViewingKey {
            key: viewing_key_addr1.clone(),
            address: ADDR1.to_string(),
        },
        Some(prev_height),
    );
    assert_eq!(resp.power, Uint128::new(100));

    // For current height, total 150, ADDR1 100, ADDR2 50
    // Total power is 150
    let resp = get_total_power_at_height(&mut app, staking_info.clone(), None);
    assert_eq!(resp.power, Uint128::new(150));

    // ADDR1 has 100 power
    let resp = get_voting_power_at_height(
        &mut app,
        staking_info.clone(),
        Auth::ViewingKey {
            key: viewing_key_addr1.clone(),
            address: ADDR1.to_string(),
        },
        None,
    );
    assert_eq!(resp.power, Uint128::new(100));

    // ADDR2 now has 50 power
    let resp = get_voting_power_at_height(
        &mut app,
        staking_info.clone(),
        Auth::ViewingKey {
            key: viewing_key_addr2.clone(),
            address: ADDR2.to_string(),
        },
        None,
    );
    assert_eq!(resp.power, Uint128::new(50));

    // ADDR1 unstakes half
    unstake_tokens(
        &mut app,
        staking_info.clone(),
        ADDR1,
        50,
        Auth::ViewingKey {
            key: viewing_key_addr1.clone(),
            address: ADDR1.to_string(),
        },
    )
    .unwrap();
    app.update_block(next_block);
    let prev_height = app.block_info().height - 2;

    // Query the previous height, total 150, ADDR1 100, ADDR2 50
    // Total power is 100
    let resp = get_total_power_at_height(&mut app, staking_info.clone(), Some(prev_height));
    assert_eq!(resp.power, Uint128::new(150));

    // ADDR1 has 100 power
    let resp = get_voting_power_at_height(
        &mut app,
        staking_info.clone(),
        Auth::ViewingKey {
            key: viewing_key_addr1.clone(),
            address: ADDR1.to_string(),
        },
        Some(prev_height),
    );
    assert_eq!(resp.power, Uint128::new(50));

    // ADDR2 still has 0 power
    let resp = get_voting_power_at_height(
        &mut app,
        staking_info.clone(),
        Auth::ViewingKey {
            key: viewing_key_addr2.clone(),
            address: ADDR2.to_string(),
        },
        Some(prev_height),
    );
    assert_eq!(resp.power, Uint128::new(50));

    // For current height, total 100, ADDR1 50, ADDR2 50
    // Total power is 100
    let resp = get_total_power_at_height(&mut app, staking_info.clone(), None);
    assert_eq!(resp.power, Uint128::new(100));

    // ADDR1 has 50 power
    let resp = get_voting_power_at_height(
        &mut app,
        staking_info.clone(),
        Auth::ViewingKey {
            key: viewing_key_addr1.clone(),
            address: ADDR1.to_string(),
        },
        None,
    );
    assert_eq!(resp.power, Uint128::new(50));

    // ADDR2 now has 50 power
    let resp = get_voting_power_at_height(
        &mut app,
        staking_info,
        Auth::ViewingKey {
            key: viewing_key_addr2.clone(),
            address: ADDR2.to_string(),
        },
        None,
    );
    assert_eq!(resp.power, Uint128::new(50));
}

#[test]
fn test_active_threshold_none() {
    let mut app = App::default();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    // Active as no threshold
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            staking_info.code_hash,
            staking_info.address.to_string(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(is_active.active);
}

#[test]
#[should_panic(
    expected = "Active threshold percentage must be greater than 0 and not greater than 1"
)]
fn test_active_threshold_percentage_gt_100() {
    let mut app = App::default();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    // Populated fields
    instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: Some(ActiveThreshold::Percentage {
                percent: Decimal::percent(120),
            }),
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );
}

#[test]
#[should_panic(
    expected = "Active threshold percentage must be greater than 0 and not greater than 1"
)]
fn test_active_threshold_percentage_lte_0() {
    let mut app = App::default();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    // Populated fields
    instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: Some(ActiveThreshold::Percentage {
                percent: Decimal::percent(0),
            }),
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );
}

#[test]
fn test_add_remove_hooks() {
    let mut app = App::default();

    let staking_contract_instantiate_info = app.store_code(staking_contract());
    let query_auth = instantiate_query_auth(&mut app);
    // Populated fields
    let staking_info = instantiate_staking(
        &mut app,
        staking_contract_instantiate_info.clone(),
        InstantiateMsg {
            token_info: TokenInfo::Existing {
                denom: DENOM.to_string(),
            },
            unstaking_duration: Some(Duration::Height(5)),
            active_threshold: None,
            query_auth: Some(RawContract {
                address: query_auth.clone().address.to_string(),
                code_hash: query_auth.clone().code_hash,
            }),
        },
    );

    // Add a hook.
    app.execute_contract(
        Addr::unchecked(DAO_ADDR),
        &staking_info.clone(),
        &ExecuteMsg::AddHook {
            addr: "hook".to_string(),
            code_hash: "hook".to_string(),
        },
        &[],
    )
    .unwrap();

    // Remove hook.
    app.execute_contract(
        Addr::unchecked(DAO_ADDR),
        &staking_info.clone(),
        &ExecuteMsg::RemoveHook {
            addr: "hook".to_string(),
            code_hash: "hook".to_string(),
        },
        &[],
    )
    .unwrap();
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
