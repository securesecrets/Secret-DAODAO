use cosmwasm_std::{testing::mock_info, Addr, ContractInfo};
use dao_voting::multiple_choice::MultipleChoiceVote;
use secret_multi_test::{App, Executor};
use shade_protocol::{basic_staking::Auth, utils::asset::RawContract};

use crate::testing::{
    contracts::proposal_single_contract,
    execute::{
        add_proposal_hook, add_proposal_hook_should_fail, add_vote_hook, add_vote_hook_should_fail,
        close_proposal_should_fail, execute_proposal_should_fail, make_proposal,
        remove_proposal_hook, remove_proposal_hook_should_fail, remove_vote_hook,
        remove_vote_hook_should_fail, update_rationale, vote_on_proposal_should_fail,
    },
    instantiate::get_default_token_dao_proposal_module_instantiate,
};

use super::{
    execute::{
        create_viewing_key, execute_veto_fails, update_config, update_config_should_fail,
        update_pre_propose_info, update_pre_propose_info_should_fail,
    },
    instantiate::instantiate_query_auth,
    CREATOR_ADDR, DAO_ADDR,
};

// The testcases fails for success a we need whole dao dao flow for this
// and due to different implementations for submsg on scrt network ... we had
// to manually add a function called parse_reply_event_for_address to get deployed
// contract addresses from events which is not working in testcases ...
// So only testcases for failure is covered.

struct CommonTest {
    app: App,
    proposal_multiple_contract_info: ContractInfo,
}
fn setup_test(sender: &str) -> CommonTest {
    let mut app = App::default();
    let proposal_module_contract_info = app.store_code(proposal_single_contract());
    let query_auth = instantiate_query_auth(&mut app);
    let instantiate = get_default_token_dao_proposal_module_instantiate(
        RawContract {
            address: query_auth.clone().address.to_string(),
            code_hash: query_auth.clone().code_hash,
        }    );
    let proposal_multiple_contract_info = app
        .instantiate_contract(
            proposal_module_contract_info,
            Addr::unchecked(sender),
            &instantiate,
            &[],
            "proposal_single",
            None,
        )
        .unwrap();

    CommonTest {
        app,
        proposal_multiple_contract_info,
    }
}

#[test]
fn test_simple_instantiate_proposal() {
    let CommonTest {
        app: _,
        proposal_multiple_contract_info: _,
    } = setup_test(DAO_ADDR);
}

// this will fail currently  as custom function parse_reply_get_contract_address from event is not woring
// for testing and we can't create a normal dao due to it.
#[test]
#[should_panic(
    expected = "Generic error: Querier contract error: secret_multi_test::multi::wasm::ContractData not found"
)]
fn test_create_proposal_without_voting_module_will_fail() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    let query_auth = instantiate_query_auth(&mut app);
    let viewing_key_creator =
        create_viewing_key(&mut app, query_auth.clone(), mock_info(CREATOR_ADDR, &[]));

    // Create Proposal
    make_proposal(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        Auth::ViewingKey {
            key: viewing_key_creator,
            address: CREATOR_ADDR.to_string(),
        },
        vec![],
    );
}

#[test]
fn test_vote_on_proposal_with_invalid_proposal_id_will_fail() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    let query_auth = instantiate_query_auth(&mut app);
    let viewing_key_creator =
        create_viewing_key(&mut app, query_auth.clone(), mock_info(CREATOR_ADDR, &[]));

    // vote on  Proposal will fail
    vote_on_proposal_should_fail(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        Auth::ViewingKey {
            key: viewing_key_creator,
            address: CREATOR_ADDR.to_string(),
        },
        1,
        MultipleChoiceVote { option_id: 1 },
    );
}

#[test]
#[should_panic(expected = "No vote exists for proposal (1) and voter (creator)")]
fn test_update_rational_fails_for_no_proposal() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // update rational fais for no proposal
    update_rationale(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        CREATOR_ADDR,
        1,
        Some("new_rational".into()),
    );
}

#[test]
fn execute_fails_on_proposal_with_invalid_proposal() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    let query_auth = instantiate_query_auth(&mut app);
    let viewing_key_creator =
        create_viewing_key(&mut app, query_auth.clone(), mock_info(CREATOR_ADDR, &[]));

    // execute on  Proposal will fail
    execute_proposal_should_fail(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        Auth::ViewingKey {
            key: viewing_key_creator,
            address: CREATOR_ADDR.to_string(),
        },
        1,
    );
}

#[test]
fn execute_veto_fails_on_proposal_with_invalid_proposal() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // veto on  Proposal will fail
    execute_veto_fails(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        CREATOR_ADDR,
        1,
    );
}

#[test]
fn close_proposal_fails_on_proposal_with_invalid_proposal() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // close on  Proposal will fail
    close_proposal_should_fail(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        CREATOR_ADDR,
        1,
    );
}

#[test]
fn update_config_works() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // update config
    update_config(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        DAO_ADDR,
    );
}

#[test]
fn update_config_fails_for_invalid_sender() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // update config fails
    update_config_should_fail(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        CREATOR_ADDR,
    );
}

#[test]
fn update_pre_propose_info_works() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // update pre-propose
    update_pre_propose_info(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        DAO_ADDR,
    );
}

#[test]
fn update_pre_propose_info_fails_for_invalid_sender() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // update pre-propose fails
    update_pre_propose_info_should_fail(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        CREATOR_ADDR,
    );
}

#[test]
fn add_proposal_hook_works() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // add proposal hook
    add_proposal_hook(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        DAO_ADDR,
        "hook_addr",
        "hook_code_hash",
    );
}

#[test]
fn add_proposal_hook_fails_for_invalid_sender() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // add proposal hook fails
    add_proposal_hook_should_fail(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        CREATOR_ADDR,
        "hook_addr",
        "hook_code_hash",
    );
}

#[test]
fn remove_proposal_hook_works() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // add proposal hook
    add_proposal_hook(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.clone().code_hash,
        DAO_ADDR,
        "hook_addr",
        "hook_code_hash",
    );

    // remove proposal hook
    remove_proposal_hook(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        DAO_ADDR,
        "hook_addr",
        "hook_code_hash",
    );
}

#[test]
fn remove_proposal_hook_fails_for_invalid_sender() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // remove proposal hook fails
    remove_proposal_hook_should_fail(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        CREATOR_ADDR,
        "hook_addr",
        "hook_code_hash",
    );
}

#[test]
fn remove_proposal_hook_fails_for_no_proposal_hook() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // remove proposal hook fails
    remove_proposal_hook_should_fail(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        DAO_ADDR,
        "hook_addr",
        "hook_code_hash",
    );
}

#[test]
fn add_vote_hook_works() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // add vote hook
    add_vote_hook(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        DAO_ADDR,
        "vote_hook_addr",
        "vote_hook_code_hash",
    );
}

#[test]
fn add_vote_hook_fails_for_invalid_sender() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // add vote hook fails
    add_vote_hook_should_fail(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        CREATOR_ADDR,
        "vote_hook_addr",
        "vote_hook_code_hash",
    );
}

#[test]
fn remove_vote_hook_works() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // add vote hook
    add_vote_hook(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.clone().code_hash,
        DAO_ADDR,
        "vote_hook_addr",
        "vote_hook_code_hash",
    );

    // remove vote hook
    remove_vote_hook(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        DAO_ADDR,
        "vote_hook_addr",
        "vote_hook_code_hash",
    );
}

#[test]
fn remove_vote_hook_fails_for_invalid_sender() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // remove vote hook fails
    remove_vote_hook_should_fail(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        CREATOR_ADDR,
        "vote_hook_addr",
        "vote_hook_code_hash",
    );
}

#[test]
fn remove_vote_hook_fails_for_no_proposal_hook() {
    let CommonTest {
        mut app,
        proposal_multiple_contract_info,
    } = setup_test(DAO_ADDR);

    // remove vote hook fails
    remove_vote_hook_should_fail(
        &mut app,
        &proposal_multiple_contract_info.address,
        proposal_multiple_contract_info.code_hash,
        DAO_ADDR,
        "vote_hook_addr",
        "vote_hook_code_hash",
    );
}
