use cosmwasm_std::{Addr, ContractInfo, Uint128};
use dao_snip721_extensions::roles::MetadataExt;
use secret_multi_test::{App, Executor};
use shade_protocol::basic_staking::Auth;

use crate::{
    msg::{InstantiateMsg, NftMintMsg, NftRolesContract},
    state::Config,
    testing::{
        dao_voting_snip721_roles_contract,
        execute::{create_viewing_key, mint_nft},
        queries::{query_config, query_info, query_minter, query_total_power, query_voting_power},
    },
};

use super::{instantiate::instantiate_snip721_roles, setup_test, CommonTest, CREATOR_ADDR};

#[test]
fn test_info_query_works() -> anyhow::Result<()> {
    let CommonTest {
        app, module_info, ..
    } = setup_test(vec![NftMintMsg {
        token_id: "1".to_string(),
        owner: CREATOR_ADDR.to_string(),
        token_uri: None,
        extension: MetadataExt {
            role: None,
            weight: 1,
        },
    }]);
    let info = query_info(&app, module_info)?;
    assert_eq!(info.info.version, env!("CARGO_PKG_VERSION").to_string());
    Ok(())
}

#[test]
#[should_panic(expected = "New snip721-roles contract must be instantiated with at least one NFT")]
fn test_instantiate_no_roles_fails() {
    setup_test(vec![]);
}

#[test]
fn test_use_existing_nft_contract() {
    let mut app = App::default();
    let module_id = app.store_code(dao_voting_snip721_roles_contract());

    let (snip721_info, _, query_auth) = instantiate_snip721_roles(&mut app, CREATOR_ADDR);
    let module_info = app
        .instantiate_contract(
            module_id,
            Addr::unchecked(CREATOR_ADDR),
            &InstantiateMsg {
                nft_contract: NftRolesContract::Existing {
                    address: snip721_info.address.clone().to_string(),
                },
            },
            &[],
            "cw721_voting",
            None,
        )
        .unwrap();

    // Get total power
    let total = query_total_power(&app, module_info.clone(), None).unwrap();
    assert_eq!(total.power, Uint128::zero());

    // Creator mints themselves a new NFT
    mint_nft(
        &mut app,
        snip721_info.clone(),
        CREATOR_ADDR,
        Some(CREATOR_ADDR.to_string()),
        Some("1".to_string()),
    )
    .unwrap();

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.address,
            code_hash: query_auth.code_hash,
        },
        CREATOR_ADDR,
    );

    // Get voting power for creator
    let vp = query_voting_power(
        &app,
        module_info,
        Auth::ViewingKey {
            key: viewing_key,
            address: CREATOR_ADDR.into(),
        },
        None,
    )
    .unwrap();
    assert_eq!(vp.power, Uint128::new(1));
}

#[test]
fn test_voting_queries() {
    let CommonTest {
        mut app,
        module_info,
        query_auth,
        ..
    } = setup_test(vec![NftMintMsg {
        token_id: "1".to_string(),
        owner: CREATOR_ADDR.to_string(),
        token_uri: None,
        extension: MetadataExt {
            role: Some("admin".to_string()),
            weight: 1,
        },
    }]);

    // Get config
    let config: Config = query_config(&app, module_info.clone()).unwrap();
    let snip721_addr = config.nft_address;
    let snip721_code_hash = config.nft_code_hash;

    // Get NFT minter
    let minter = query_minter(
        &app,
        ContractInfo {
            address: snip721_addr.clone(),
            code_hash: snip721_code_hash.clone(),
        },
    )
    .unwrap();
    // Minter should be the contract that instantiated the cw721 contract.
    // In the test setup, this is the module_addr but would normally be
    // the dao-core contract.
    assert_eq!(minter.minters, vec![module_info.address.clone()]);

    // Get total power
    let total = query_total_power(&app, module_info.clone(), None).unwrap();
    assert_eq!(total.power, Uint128::new(1));

    let viewing_key = create_viewing_key(
        &mut app,
        ContractInfo {
            address: query_auth.address,
            code_hash: query_auth.code_hash,
        },
        CREATOR_ADDR,
    );
    // Get voting power for creator
    let vp = query_voting_power(
        &app,
        module_info.clone(),
        Auth::ViewingKey {
            key: viewing_key.clone(),
            address: CREATOR_ADDR.into(),
        },
        None,
    )
    .unwrap();
    assert_eq!(vp.power, Uint128::new(1));

    // Mint a new NFT
    mint_nft(
        &mut app,
        ContractInfo {
            address: snip721_addr,
            code_hash: snip721_code_hash,
        },
        module_info.address.as_ref(),
        Some(CREATOR_ADDR.to_string()),
        Some("2".to_string()),
    )
    .unwrap();

    // Get total power
    let total = query_total_power(&app, module_info.clone(), None).unwrap();
    assert_eq!(total.power, Uint128::new(2));

    // Get voting power for creator
    let vp = query_voting_power(
        &app,
        module_info,
        Auth::ViewingKey {
            key: viewing_key,
            address: CREATOR_ADDR.into(),
        },
        None,
    )
    .unwrap();
    assert_eq!(vp.power, Uint128::new(2));
}
