use cosmwasm_std::Uint128;
use secret_multi_test::next_block;
use shade_protocol::basic_staking::Auth;

use crate::testing::{
    execute::{create_viewing_key, stake_nft, unstake_nfts},
    instantiate::instantiate_snip721_base,
    queries::query_voting_power,
};

use super::{
    execute::mint_and_stake_nft, is_error, queries::query_total_and_voting_power, setup_test,
    CommonTest, CREATOR_ADDR,
};

#[test]
fn test_stake_and_unstake() -> anyhow::Result<()> {
    let CommonTest {
        mut app,
        module,
        nft,
        query_auth,
    } = setup_test(None);

    let viewing_key = create_viewing_key(&mut app, query_auth, CREATOR_ADDR);

    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "1")?;
    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "2")?;

    app.update_block(next_block);

    let (total, voting) = query_total_and_voting_power(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
        None,
    )?;
    assert_eq!(total, Uint128::new(2));
    assert_eq!(voting, Uint128::new(2));

    unstake_nfts(&mut app, &module, CREATOR_ADDR, &["1", "2"])?;

    // changed,
    let (total, voting) = query_total_and_voting_power(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
        None,
    )?;
    assert_eq!(total, Uint128::new(0));
    assert_eq!(voting, Uint128::new(0));

    app.update_block(next_block);

    stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "1")?;
    stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "2")?;

    // changed.
    let (total, voting) = query_total_and_voting_power(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
        None,
    )?;
    assert_eq!(total, Uint128::new(2));
    assert_eq!(voting, Uint128::new(2));

    app.update_block(next_block);

    // Still unchanged.
    let (total, voting) = query_total_and_voting_power(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
        None,
    )?;
    assert_eq!(total, Uint128::new(2));
    assert_eq!(voting, Uint128::new(2));

    Ok(())
}

/// I can not stake NFTs from a collection other than the one this has
/// been configured for.
#[test]
fn test_stake_wrong_nft() -> anyhow::Result<()> {
    let CommonTest {
        mut app,
        module,
        query_auth,
        ..
    } = setup_test(None);

    let viewing_key = create_viewing_key(&mut app, query_auth, CREATOR_ADDR);

    let other_nft = instantiate_snip721_base(&mut app, CREATOR_ADDR, CREATOR_ADDR);

    let res = mint_and_stake_nft(&mut app, &other_nft, &module, CREATOR_ADDR, "1");
    is_error!(res => "Invalid token.");

    app.update_block(next_block);
    let voting = query_voting_power(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
        None,
    )?;
    assert_eq!(voting.power, Uint128::new(0));

    Ok(())
}

/// I can determine what my voting power _will_ be after staking by
/// asking for my voting power one block in the future.
#[test]
fn test_query_the_future() -> anyhow::Result<()> {
    let CommonTest {
        mut app,
        module,
        nft,
        query_auth,
    } = setup_test(None);

    let viewing_key = create_viewing_key(&mut app, query_auth, CREATOR_ADDR);

    mint_and_stake_nft(&mut app, &nft, &module, CREATOR_ADDR, "1")?;

    // Future voting power will be one under current conditions.
    let voting = query_voting_power(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
        Some(app.block_info().height + 100),
    )?;
    assert_eq!(voting.power, Uint128::new(1));

    app.update_block(next_block);

    // Current voting power is 1.
    let voting = query_voting_power(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
        None,
    )?;
    assert_eq!(voting.power, Uint128::new(1));

    unstake_nfts(&mut app, &module, CREATOR_ADDR, &["1"])?;

    // Future voting power is now zero.
    let voting = query_voting_power(
        &app,
        &module,
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.to_string().clone(),
        },
        Some(app.block_info().height + 100),
    )?;
    assert_eq!(voting.power, Uint128::zero());

    Ok(())
}
