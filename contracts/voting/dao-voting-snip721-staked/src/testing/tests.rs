use cosmwasm_std::testing::{mock_dependencies, mock_env};
use cosmwasm_std::{to_binary, Addr, ContractInfo, Decimal, Uint128};
use dao_interface::voting::IsActiveResponse;
use dao_voting::threshold::{ActiveThreshold, ActiveThresholdResponse};
use secret_cw_controllers::HookItem;
use secret_multi_test::{next_block, App, Executor};
use secret_utils::Duration;
use shade_protocol::basic_staking::Auth;
use shade_protocol::utils::asset::RawContract;
use snip721_controllers::{NftClaim, NftClaimsResponse};
use snip721_reference_impl::msg::{
    ExecuteMsg as Snip721ExecuteMsg, InstantiateMsg as Snip721InstantiateMsg,
};

use crate::testing::execute::create_viewing_key;
use crate::testing::instantiate::instantiate_query_auth;
use crate::{
    contract::{migrate, CONTRACT_NAME, CONTRACT_VERSION},
    msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, NftContract, QueryMsg},
    state::MAX_CLAIMS,
    testing::{
        execute::{
            claim_nfts, mint_and_stake_nft, mint_nft, stake_nft, unstake_nfts, update_config,
        },
        queries::{query_config, query_hooks, query_nft_owner, query_total_and_voting_power},
    },
};

use super::instantiate::{
    instantiate_snip721_base, snip721_base_contract, voting_snip721_staked_contract,
};
use super::{
    execute::{add_hook, remove_hook},
    is_error,
    queries::{query_claims, query_info, query_staked_nfts, query_total_power, query_voting_power},
    setup_test, CommonTest, CREATOR_ADDR,
};

// I can create new NFT collection when creating a dao-voting-snip721-staked contract
#[test]
fn test_instantiate_with_new_cw721_collection() -> anyhow::Result<()> {
    let mut app = App::default();
    let module_instantiation_info = app.store_code(voting_snip721_staked_contract());
    let nft = instantiate_snip721_base(&mut app, CREATOR_ADDR, CREATOR_ADDR);
    let query_auth = instantiate_query_auth(&mut app, CREATOR_ADDR);

    let module_addr = app
        .instantiate_contract(
            module_instantiation_info,
            Addr::unchecked(CREATOR_ADDR),
            &InstantiateMsg {
                nft_contract: NftContract::Existing {
                    address: nft.address.to_string(),
                    code_hash: nft.code_hash.clone(),
                },
                unstaking_duration: None,
                active_threshold: None,
                dao_code_hash: "dao_code_hash".to_string(),
                query_auth: Some(RawContract {
                    code_hash: query_auth.code_hash.clone(),
                    address: query_auth.address.clone().to_string(),
                }),
            },
            &[],
            "snip721_voting",
            None,
        )
        .unwrap();

    let config = query_config(&app, &module_addr)?;
    let snip721_addr = config.nft_address;
    let snip721_code_hash = config.nft_code_hash;

    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "1").unwrap();
    // Check that the NFT contract was created
    let owner = query_nft_owner(
        &app,
        &ContractInfo {
            address: snip721_addr,
            code_hash: snip721_code_hash,
        },
        "1",
    )?;
    assert_eq!(owner.owner, CREATOR_ADDR);

    Ok(())
}

// I can stake tokens, voting power and total power is updated one
// block later.
#[test]
fn test_stake_tokens() -> anyhow::Result<()> {
    let CommonTest {
        mut app,
        module,
        nft,
        query_auth,
    } = setup_test(None);

    let viewing_key = create_viewing_key(&mut app, query_auth, CREATOR_ADDR);

    let total_power = query_total_power(&app, &module, None)?;
    let voting_power = query_voting_power(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
        None,
    )?;

    assert_eq!(total_power.power, Uint128::zero());
    assert_eq!(total_power.height, app.block_info().height);

    assert_eq!(voting_power.power, Uint128::zero());
    assert_eq!(voting_power.height, app.block_info().height);

    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "1")?;

    app.update_block(next_block);

    let (total, personal) = query_total_and_voting_power(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
        None,
    )?;
    assert_eq!(total, Uint128::new(1));
    assert_eq!(personal, Uint128::new(1));

    Ok(())
}

// I can unstake tokens. Unstaking more than one token at once
// works. I can not unstake a token more than once. I can not unstake
// another addresses' token. Voting power and total power is updated
// when I unstake.
#[test]
fn test_unstake_tokens_no_claims() -> anyhow::Result<()> {
    let CommonTest {
        mut app,
        module,
        nft,
        query_auth,
    } = setup_test(None);

    let viewing_key = create_viewing_key(&mut app, query_auth, CREATOR_ADDR);

    let friend = "friend";

    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "1")?;
    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "2")?;
    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "3")?;

    mint_nft(&mut app, &nft, CREATOR_ADDR, friend, "4")?;
    mint_nft(&mut app, &nft, CREATOR_ADDR, friend, "5")?;
    app.update_block(next_block);
    stake_nft(&mut app, &nft, &module, friend, "4")?;
    stake_nft(&mut app, &nft, &module, friend, "5")?;

    app.update_block(next_block);

    let (total, personal) = query_total_and_voting_power(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
        None,
    )?;
    assert_eq!(total, Uint128::new(5));
    assert_eq!(personal, Uint128::new(3));

    unstake_nfts(&mut app, &module, CREATOR_ADDR, &["1", "2"])?;

    // Voting power is updated when I unstake. Waits a block as it's a
    // snapshot map.
    app.update_block(next_block);
    let (total, personal) = query_total_and_voting_power(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
        None,
    )?;
    assert_eq!(total, Uint128::new(3));
    assert_eq!(personal, Uint128::new(1));

    // I can not unstake tokens I do not own. Anyhow can't figure out
    // how to downcast this error so we check for the expected string.
    let res = unstake_nfts(&mut app, &module, CREATOR_ADDR, &["4"]);
    is_error!(res => "Can not unstake that which you have not staked (unstaking 4)");

    let res = unstake_nfts(&mut app, &module, CREATOR_ADDR, &["5", "4"]);
    is_error!(res => "Can not unstake that which you have not staked (unstaking 5)");

    let res = unstake_nfts(&mut app, &module, CREATOR_ADDR, &["☯️", "4"]);
    is_error!(res => "Can not unstake that which you have not staked (unstaking ☯️)");

    // I can not unstake tokens more than once.
    let res = unstake_nfts(&mut app, &module, CREATOR_ADDR, &["1"]);
    is_error!(res => "Can not unstake that which you have not staked (unstaking 1)");

    Ok(())
}

// I can update the unstaking duration and the owner. Only the owner
// may do this. I can unset the owner. Updating the unstaking duration
// does not impact outstanding claims.
#[test]
fn test_update_config() -> anyhow::Result<()> {
    let CommonTest {
        mut app,
        module,
        nft,
        query_auth,
    } = setup_test(Some(Duration::Height(3)));

    let viewing_key = create_viewing_key(&mut app, query_auth, CREATOR_ADDR);

    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "1")?;
    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "2")?;

    unstake_nfts(&mut app, &module, CREATOR_ADDR, &["1"])?;

    let claims = query_claims(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
    )?;
    assert_eq!(
        claims,
        NftClaimsResponse {
            nft_claims: vec![NftClaim {
                token_id: "1".to_string(),
                release_at: secret_utils::Expiration::AtHeight(app.block_info().height + 3)
            }]
        }
    );

    // Update config to invalid duration fails
    let err = update_config(&mut app, &module, CREATOR_ADDR, Some(Duration::Time(0))).unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Invalid unstaking duration, unstaking duration cannot be 0".to_string()
    );

    // Update duration
    update_config(&mut app, &module, CREATOR_ADDR, Some(Duration::Time(1)))?;

    // Existing claims should remain unchanged.
    let claims = query_claims(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
    )?;
    assert_eq!(
        claims,
        NftClaimsResponse {
            nft_claims: vec![NftClaim {
                token_id: "1".to_string(),
                release_at: secret_utils::Expiration::AtHeight(app.block_info().height + 3)
            }]
        }
    );

    // New claims should reflect the new unstaking duration. Old ones
    // should not.
    unstake_nfts(&mut app, &module, CREATOR_ADDR, &["2"])?;
    let claims = query_claims(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
    )?;
    assert_eq!(
        claims,
        NftClaimsResponse {
            nft_claims: vec![
                NftClaim {
                    token_id: "1".to_string(),
                    release_at: secret_utils::Expiration::AtHeight(app.block_info().height + 3)
                },
                NftClaim {
                    token_id: "2".to_string(),
                    release_at: Duration::Time(1).after(&app.block_info())
                }
            ]
        }
    );

    let info = app.block_info();
    app.update_block(|block| {
        block.height += 3;
        block.time = match Duration::Time(1).after(&info) {
            secret_utils::Expiration::AtTime(timestamp) => timestamp,
            _ => panic!("there should really be an easier way to do this"),
        }
    });

    // Do a claim for good measure.
    claim_nfts(&mut app, &module, CREATOR_ADDR)?;
    let claims = query_claims(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
    )?;
    assert_eq!(claims, NftClaimsResponse { nft_claims: vec![] });

    Ok(())
}

// I can query my pending claims. Attempting to claim with nothing to
// claim results in an error. Attempting to claim with tokens to claim
// results in me owning those tokens.
#[test]
fn test_claims() -> anyhow::Result<()> {
    let CommonTest {
        mut app,
        module,
        nft,
        query_auth,
    } = setup_test(Some(Duration::Height(1)));

    let viewing_key = create_viewing_key(&mut app, query_auth, CREATOR_ADDR);

    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "1")?;
    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "2")?;
    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "3")?;

    let claims = query_claims(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
    )?;
    assert_eq!(claims.nft_claims, vec![]);

    let res = claim_nfts(&mut app, &module, CREATOR_ADDR);
    is_error!(res => "Nothing to claim");

    unstake_nfts(&mut app, &module, CREATOR_ADDR, &["2"])?;

    let claims = query_claims(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
    )?;
    assert_eq!(
        claims.nft_claims,
        vec![NftClaim {
            token_id: "2".to_string(),
            release_at: secret_utils::Expiration::AtHeight(app.block_info().height + 1)
        }]
    );

    // Claim now exists, but is not yet expired. Nothing to claim.
    let res = claim_nfts(&mut app, &module, CREATOR_ADDR);
    is_error!(res => "Nothing to claim");

    app.update_block(next_block);
    claim_nfts(&mut app, &module, CREATOR_ADDR)?;

    let owner = query_nft_owner(&app, &nft, "2")?;
    assert_eq!(owner.owner, CREATOR_ADDR.to_string());

    Ok(())
}

// I can not have more than MAX_CLAIMS claims pending.
#[test]
fn test_max_claims() -> anyhow::Result<()> {
    let CommonTest {
        mut app,
        module,
        nft,
        ..
    } = setup_test(Some(Duration::Height(1)));

    for i in 0..MAX_CLAIMS {
        let i_str = &i.to_string();
        mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, i_str)?;
        app.update_block(next_block);
        unstake_nfts(&mut app, &module, CREATOR_ADDR, &[i_str])?;
    }
    app.update_block(next_block);
    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "a")?;
    let res = unstake_nfts(&mut app, &module, CREATOR_ADDR, &["a"]);
    is_error!(res => "Too many outstanding claims. Claim some tokens before unstaking more.");

    Ok(())
}

// I can list all of the currently staked NFTs for an address.
#[test]
fn test_list_staked_nfts() -> anyhow::Result<()> {
    let CommonTest {
        mut app,
        module,
        nft,
        query_auth,
    } = setup_test(Some(Duration::Height(1)));

    let viewing_key = create_viewing_key(&mut app, query_auth.clone(), CREATOR_ADDR);

    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "1")?;
    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "2")?;
    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "3")?;
    app.update_block(next_block);

    let deardrie = "deardrie";
    let viewing_key_deardrie = create_viewing_key(&mut app, query_auth.clone(), deardrie);

    mint_nft(&mut app, &nft, CREATOR_ADDR, deardrie, "4")?;
    mint_nft(&mut app, &nft, CREATOR_ADDR, deardrie, "5")?;
    app.update_block(next_block);

    let nfts = query_staked_nfts(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key_deardrie.clone(),
            address: deardrie.to_string().clone(),
        },
    )?;
    assert_eq!(nfts, Uint128::zero());

    stake_nft(&mut app, &nft, &module, deardrie, "4")?;
    stake_nft(&mut app, &nft, &module, deardrie, "5")?;

    let nfts = query_staked_nfts(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key_deardrie.clone(),
            address: deardrie.to_string().clone(),
        },
    )?;
    assert_eq!(nfts, Uint128::new(2));

    let nfts = query_staked_nfts(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
    )?;
    assert_eq!(nfts, Uint128::new(3));

    unstake_nfts(&mut app, &module, CREATOR_ADDR, &["2"])?;
    app.update_block(next_block);
    let nfts = query_staked_nfts(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
    )?;
    assert_eq!(nfts, Uint128::new(2));

    Ok(())
}

#[test]
fn test_info_query_works() -> anyhow::Result<()> {
    let CommonTest { app, module, .. } = setup_test(None);
    let info = query_info(&app, &module)?;
    assert_eq!(info.info.version, env!("CARGO_PKG_VERSION").to_string());
    Ok(())
}

// The owner may add and remove hooks.
#[test]
fn test_add_remove_hooks() -> anyhow::Result<()> {
    let CommonTest {
        mut app,
        module,
        nft,
        ..
    } = setup_test(None);

    add_hook(
        &mut app,
        &module,
        CREATOR_ADDR,
        "meow",
        "meow_code_hash".to_string(),
    )?;
    remove_hook(
        &mut app,
        &module,
        CREATOR_ADDR,
        "meow",
        "meow_code_hash".to_string(),
    )?;

    // Minting NFT works if no hooks
    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "1").unwrap();

    // Add a hook to a fake contract called "meow"
    add_hook(
        &mut app,
        &module,
        CREATOR_ADDR,
        "meow",
        "meow_code_hash".to_string(),
    )?;

    let hooks = query_hooks(&app, &module)?;
    assert_eq!(
        hooks.hooks,
        vec![HookItem {
            addr: Addr::unchecked("meow"),
            code_hash: "meow_code_hash".to_string(),
        }]
    );

    // Minting / staking now doesn't work because meow isn't a contract
    // This failure means the hook is working
    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "1").unwrap_err();

    let res = add_hook(
        &mut app,
        &module,
        CREATOR_ADDR,
        "meow",
        "meow_code_hash".to_string(),
    );
    is_error!(res => "Given address already registered as a hook");

    let res = remove_hook(
        &mut app,
        &module,
        CREATOR_ADDR,
        "blue",
        "blue_code_hash".to_string(),
    );
    is_error!(res => "Given address not registered as a hook");

    let res = add_hook(
        &mut app,
        &module,
        "ekez",
        "evil",
        "evil_code_hash".to_string(),
    );
    is_error!(res => "Unauthorized");

    Ok(())
}

#[test]
fn test_instantiate_with_invalid_duration_fails() {
    let mut app = App::default();
    let module_instantiation_info = app.store_code(voting_snip721_staked_contract());
    let snip721_iinstantiation_info = app.store_code(snip721_base_contract());

    let err = app
        .instantiate_contract(
            module_instantiation_info,
            Addr::unchecked(CREATOR_ADDR),
            &InstantiateMsg {
                nft_contract: NftContract::New {
                    code_id: snip721_iinstantiation_info.code_id,
                    code_hash: snip721_iinstantiation_info.code_hash,
                    label: "Test NFT".to_string(),
                    msg: to_binary(&Snip721InstantiateMsg {
                        name: "Test NFT".to_string(),
                        symbol: "TEST".to_string(),
                        admin: None,
                        entropy: "entropy".to_string(),
                        royalty_info: None,
                        config: None,
                        post_init_callback: None,
                    })
                    .unwrap(),
                    initial_nfts: vec![],
                },
                unstaking_duration: None,
                active_threshold: None,
                query_auth: None,
                dao_code_hash: "".to_string(),
            },
            &[],
            "snip721_voting",
            None,
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "New NFT contract must be instantiated with at least one NFT".to_string()
    );
}

#[test]
#[should_panic(expected = "Error instantiating NFT contract")]
fn test_instantiate_zero_active_threshold_count() {
    let mut app = App::default();
    let module_instantiation_info = app.store_code(voting_snip721_staked_contract());
    let snip721_iinstantiation_info = app.store_code(snip721_base_contract());

    app.instantiate_contract(
        module_instantiation_info,
        Addr::unchecked(CREATOR_ADDR),
        &InstantiateMsg {
            nft_contract: NftContract::New {
                code_id: snip721_iinstantiation_info.code_id,
                code_hash: snip721_iinstantiation_info.code_hash,
                label: "Test NFT".to_string(),
                msg: to_binary(&Snip721InstantiateMsg {
                    name: "Test NFT".to_string(),
                    symbol: "TEST".to_string(),
                    admin: None,
                    entropy: "entropy".to_string(),
                    royalty_info: None,
                    config: None,
                    post_init_callback: None,
                })
                .unwrap(),
                initial_nfts: vec![to_binary(&Snip721ExecuteMsg::MintNft {
                    owner: Some(CREATOR_ADDR.to_string()),
                    token_id: Some("1".to_string()),
                    public_metadata: None,
                    private_metadata: None,
                    serial_number: None,
                    royalty_info: None,
                    transferable: None,
                    memo: None,
                    padding: None,
                })
                .unwrap()],
            },
            unstaking_duration: None,
            active_threshold: Some(ActiveThreshold::AbsoluteCount {
                count: Uint128::zero(),
            }),
            query_auth: None,
            dao_code_hash: "".to_string(),
        },
        &[],
        "snip721_voting",
        None,
    )
    .unwrap();
}

#[test]
#[should_panic(expected = "Absolute count threshold cannot be greater than the total token supply")]
fn test_instantiate_invalid_active_threshold_count_existing_nft() {
    let mut app = App::default();
    let module = app.store_code(voting_snip721_staked_contract());
    let nft = instantiate_snip721_base(&mut app, CREATOR_ADDR, CREATOR_ADDR);

    app.instantiate_contract(
        module,
        Addr::unchecked(CREATOR_ADDR),
        &InstantiateMsg {
            nft_contract: NftContract::Existing {
                address: nft.address.to_string(),
                code_hash: nft.code_hash,
            },
            unstaking_duration: None,
            active_threshold: Some(ActiveThreshold::AbsoluteCount {
                count: Uint128::new(100),
            }),
            query_auth: None,
            dao_code_hash: "".to_string(),
        },
        &[],
        "snip721_voting",
        None,
    )
    .unwrap();
}

#[test]
fn test_active_threshold_absolute_count() {
    let mut app = App::default();
    let module_instantiation_info = app.store_code(voting_snip721_staked_contract());
    let nft = instantiate_snip721_base(&mut app, CREATOR_ADDR, CREATOR_ADDR);
    let query_auth = instantiate_query_auth(&mut app, CREATOR_ADDR);

    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "1").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "2").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "3").unwrap();

    app.update_block(next_block);

    let module = app
        .instantiate_contract(
            module_instantiation_info,
            Addr::unchecked(CREATOR_ADDR),
            &InstantiateMsg {
                nft_contract: NftContract::Existing {
                    address: nft.address.to_string(),
                    code_hash: nft.code_hash.clone(),
                },
                unstaking_duration: None,
                active_threshold: Some(ActiveThreshold::AbsoluteCount {
                    count: Uint128::new(3),
                }),
                dao_code_hash: "dao_code_hash".to_string(),
                query_auth: Some(RawContract {
                    code_hash: query_auth.code_hash.clone(),
                    address: query_auth.address.clone().to_string(),
                }),
            },
            &[],
            "snip721_voting",
            None,
        )
        .unwrap();

    app.update_block(next_block);

    // Not active as none staked
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            module.code_hash.clone(),
            module.address.clone(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    println!("{}", is_active.active);
    assert!(!is_active.active);

    // Stake NFTs
    stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "1").unwrap();
    stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "2").unwrap();
    stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "3").unwrap();

    app.update_block(next_block);

    // Active as enough staked
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            module.code_hash.clone(),
            module.address.clone(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(is_active.active);
}

#[test]
fn test_active_threshold_percent() {
    let mut app = App::default();
    let module_instantiation_info = app.store_code(voting_snip721_staked_contract());
    let nft = instantiate_snip721_base(&mut app, CREATOR_ADDR, CREATOR_ADDR);
    let query_auth = instantiate_query_auth(&mut app, CREATOR_ADDR);

    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "1").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "2").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "3").unwrap();

    app.update_block(next_block);

    let module = app
        .instantiate_contract(
            module_instantiation_info,
            Addr::unchecked(CREATOR_ADDR),
            &InstantiateMsg {
                nft_contract: NftContract::Existing {
                    address: nft.address.to_string(),
                    code_hash: nft.code_hash.clone(),
                },
                unstaking_duration: None,
                active_threshold: Some(ActiveThreshold::Percentage {
                    percent: Decimal::percent(20),
                }),
                dao_code_hash: "dao_code_hash".to_string(),
                query_auth: Some(RawContract {
                    code_hash: query_auth.code_hash.clone(),
                    address: query_auth.address.clone().to_string(),
                }),
            },
            &[],
            "snip721_voting",
            None,
        )
        .unwrap();

    app.update_block(next_block);

    // Not active as none staked
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            module.code_hash.clone(),
            module.address.clone(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    println!("{}", is_active.active);
    assert!(!is_active.active);

    // Stake NFTs
    stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "1").unwrap();
    stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "2").unwrap();
    stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "3").unwrap();

    app.update_block(next_block);

    // Active as enough staked
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            module.code_hash.clone(),
            module.address.clone(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(is_active.active);
}

#[test]
fn test_active_threshold_percent_rounds_up() {
    let mut app = App::default();
    let module_instantiation_info = app.store_code(voting_snip721_staked_contract());
    let nft = instantiate_snip721_base(&mut app, CREATOR_ADDR, CREATOR_ADDR);
    let query_auth = instantiate_query_auth(&mut app, CREATOR_ADDR);

    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "1").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "2").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "3").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "4").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "5").unwrap();

    app.update_block(next_block);

    let module = app
        .instantiate_contract(
            module_instantiation_info,
            Addr::unchecked(CREATOR_ADDR),
            &InstantiateMsg {
                nft_contract: NftContract::Existing {
                    address: nft.address.to_string(),
                    code_hash: nft.code_hash.clone(),
                },
                unstaking_duration: None,
                active_threshold: Some(ActiveThreshold::Percentage {
                    percent: Decimal::percent(50),
                }),
                dao_code_hash: "dao_code_hash".to_string(),
                query_auth: Some(RawContract {
                    code_hash: query_auth.code_hash.clone(),
                    address: query_auth.address.clone().to_string(),
                }),
            },
            &[],
            "snip721_voting",
            None,
        )
        .unwrap();

    app.update_block(next_block);

    // Not active as none staked
    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            module.code_hash.clone(),
            module.address.clone(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(!is_active.active);

    // Stake 2 token as creator, should not be active.
    stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "1").unwrap();
    stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "2").unwrap();

    app.update_block(next_block);

    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            module.code_hash.clone(),
            module.address.clone(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(!is_active.active);

    // Stake 1 more token as creator, should now be active.
    stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "3").unwrap();
    app.update_block(next_block);

    let is_active: IsActiveResponse = app
        .wrap()
        .query_wasm_smart(
            module.code_hash.clone(),
            module.address.clone(),
            &QueryMsg::IsActive {},
        )
        .unwrap();
    assert!(is_active.active);
}

#[test]
fn test_update_active_threshold() {
    let mut app = App::default();
    let module_instantiation_info = app.store_code(voting_snip721_staked_contract());
    let nft = instantiate_snip721_base(&mut app, CREATOR_ADDR, CREATOR_ADDR);
    let query_auth = instantiate_query_auth(&mut app, CREATOR_ADDR);

    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "1").unwrap();

    app.update_block(next_block);

    let module = app
        .instantiate_contract(
            module_instantiation_info,
            Addr::unchecked(CREATOR_ADDR),
            &InstantiateMsg {
                nft_contract: NftContract::Existing {
                    address: nft.address.to_string(),
                    code_hash: nft.code_hash.clone(),
                },
                unstaking_duration: None,
                active_threshold: None,
                dao_code_hash: "dao_code_hash".to_string(),
                query_auth: Some(RawContract {
                    code_hash: query_auth.code_hash.clone(),
                    address: query_auth.address.clone().to_string(),
                }),
            },
            &[],
            "snip721_voting",
            None,
        )
        .unwrap();

    app.update_block(next_block);

    let resp: ActiveThresholdResponse = app
        .wrap()
        .query_wasm_smart(
            module.code_hash.clone(),
            module.address.clone(),
            &QueryMsg::ActiveThreshold {},
        )
        .unwrap();
    assert_eq!(resp.active_threshold, None);

    let msg = ExecuteMsg::UpdateActiveThreshold {
        new_threshold: Some(ActiveThreshold::AbsoluteCount {
            count: Uint128::new(1),
        }),
    };

    // Expect failure as sender is not the DAO
    app.execute_contract(Addr::unchecked("bob"), &module.clone(), &msg, &[])
        .unwrap_err();

    // Expect success as sender is the DAO (in this case the creator)
    app.execute_contract(Addr::unchecked(CREATOR_ADDR), &module.clone(), &msg, &[])
        .unwrap();

    let resp: ActiveThresholdResponse = app
        .wrap()
        .query_wasm_smart(
            module.code_hash.clone(),
            module.address.clone(),
            &QueryMsg::ActiveThreshold {},
        )
        .unwrap();
    assert_eq!(
        resp.active_threshold,
        Some(ActiveThreshold::AbsoluteCount {
            count: Uint128::new(1)
        })
    );
}

#[test]
#[should_panic(
    expected = "Active threshold percentage must be greater than 0 and not greater than 1"
)]
fn test_active_threshold_percentage_gt_100() {
    let mut app = App::default();
    let module_instantiation_info = app.store_code(voting_snip721_staked_contract());
    let nft = instantiate_snip721_base(&mut app, CREATOR_ADDR, CREATOR_ADDR);
    let query_auth = instantiate_query_auth(&mut app, CREATOR_ADDR);

    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "1").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "2").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "3").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "4").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "5").unwrap();

    app.update_block(next_block);

    app.instantiate_contract(
        module_instantiation_info,
        Addr::unchecked(CREATOR_ADDR),
        &InstantiateMsg {
            nft_contract: NftContract::Existing {
                address: nft.address.to_string(),
                code_hash: nft.code_hash.clone(),
            },
            unstaking_duration: None,
            active_threshold: Some(ActiveThreshold::Percentage {
                percent: Decimal::percent(120),
            }),
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: Some(RawContract {
                code_hash: query_auth.code_hash.clone(),
                address: query_auth.address.clone().to_string(),
            }),
        },
        &[],
        "snip721_voting",
        None,
    )
    .unwrap();
}

#[test]
#[should_panic(
    expected = "Active threshold percentage must be greater than 0 and not greater than 1"
)]
fn test_active_threshold_percentage_lte_0() {
    let mut app = App::default();
    let module_instantiation_info = app.store_code(voting_snip721_staked_contract());
    let nft = instantiate_snip721_base(&mut app, CREATOR_ADDR, CREATOR_ADDR);
    let query_auth = instantiate_query_auth(&mut app, CREATOR_ADDR);

    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "1").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "2").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "3").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "4").unwrap();
    mint_nft(&mut app, &nft, CREATOR_ADDR, CREATOR_ADDR, "5").unwrap();

    app.update_block(next_block);

    app.instantiate_contract(
        module_instantiation_info,
        Addr::unchecked(CREATOR_ADDR),
        &InstantiateMsg {
            nft_contract: NftContract::Existing {
                address: nft.address.to_string(),
                code_hash: nft.code_hash.clone(),
            },
            unstaking_duration: None,
            active_threshold: Some(ActiveThreshold::Percentage {
                percent: Decimal::percent(0),
            }),
            dao_code_hash: "dao_code_hash".to_string(),
            query_auth: Some(RawContract {
                code_hash: query_auth.code_hash.clone(),
                address: query_auth.address.clone().to_string(),
            }),
        },
        &[],
        "snip721_voting",
        None,
    )
    .unwrap();
}

#[test]
fn test_invalid_instantiate_msg() {
    let mut app = App::default();

    let module_instantiation_info = app.store_code(voting_snip721_staked_contract());

    let err = app
        .instantiate_contract(
            module_instantiation_info.clone(),
            Addr::unchecked(CREATOR_ADDR),
            &InstantiateMsg {
                nft_contract: NftContract::New {
                    code_id: module_instantiation_info.code_id,
                    code_hash: module_instantiation_info.code_hash,
                    label: "Test NFT".to_string(),
                    msg: to_binary(&Snip721InstantiateMsg {
                        name: "Test NFT".to_string(),
                        symbol: "TEST".to_string(),
                        admin: None,
                        entropy: "entropy".to_string(),
                        royalty_info: None,
                        config: None,
                        post_init_callback: None,
                    })
                    .unwrap(),
                    initial_nfts: vec![to_binary(&Snip721ExecuteMsg::MintNft {
                        owner: Some(CREATOR_ADDR.to_string()),
                        token_id: Some("1".to_string()),
                        public_metadata: None,
                        private_metadata: None,
                        serial_number: None,
                        royalty_info: None,
                        transferable: None,
                        memo: None,
                        padding: None,
                    })
                    .unwrap()],
                },
                unstaking_duration: None,
                active_threshold: Some(ActiveThreshold::AbsoluteCount {
                    count: Uint128::zero(),
                }),
                query_auth: None,
                dao_code_hash: "".to_string(),
            },
            &[],
            "snip721_voting",
            None,
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Error instantiating NFT contract".to_string()
    );
}

#[test]
fn test_no_initial_nfts_fails() {
    let mut app = App::default();

    let module_instantiation_info = app.store_code(voting_snip721_staked_contract());

    let err = app
        .instantiate_contract(
            module_instantiation_info.clone(),
            Addr::unchecked(CREATOR_ADDR),
            &InstantiateMsg {
                nft_contract: NftContract::New {
                    code_id: module_instantiation_info.code_id,
                    code_hash: module_instantiation_info.code_hash,
                    label: "Test NFT".to_string(),
                    msg: to_binary(&Snip721InstantiateMsg {
                        name: "Test NFT".to_string(),
                        symbol: "TEST".to_string(),
                        admin: None,
                        entropy: "entropy".to_string(),
                        royalty_info: None,
                        config: None,
                        post_init_callback: None,
                    })
                    .unwrap(),
                    initial_nfts: vec![],
                },
                unstaking_duration: None,
                active_threshold: Some(ActiveThreshold::AbsoluteCount {
                    count: Uint128::zero(),
                }),
                query_auth: None,
                dao_code_hash: "".to_string(),
            },
            &[],
            "snip721_voting",
            None,
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "New NFT contract must be instantiated with at least one NFT".to_string()
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
