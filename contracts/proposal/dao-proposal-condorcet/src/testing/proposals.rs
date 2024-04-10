use cosmwasm_std::testing::mock_info;
use secret_multi_test::App;
use shade_protocol::basic_staking::Auth;

use crate::{
    testing::{
        contracts::create_viewing_key, instantiation::instantiate_query_auth,
        suite::unimportant_message,
    },
    ContractError,
};

use super::{is_error, suite::SuiteBuilder};

// The testcases fails for success a we need whole dao dao flow for this
// and due to different implementations for submsg on scrt network ... we had
// to manually add a function called parse_reply_event_for_address to get deployed
// contract addresses from events which is not working in testcases ...
// So only testcases for failure is covered.

// a condorcet winner does not exist and the proposal is closed.
// #[test]
// fn test_proposal_lifecycle_closed() {
//     let mut suite = SuiteBuilder::default()
//         .with_voters(&[
//             ("blue", 10),
//             ("violet", 10),
//             ("magenta", 10),
//             ("gold", 10),
//             ("crimson", 10),
//             ("turquoise", 10),
//         ])
//         .with_proposal(2)
//         .build();
//     let mut app = App::default();
//     let query_auth = instantiate_query_auth(&mut app);
//     let viewing_key_blue = create_viewing_key(&mut app, query_auth.clone(), mock_info("blue", &[]));
//     let viewing_key_violet =
//         create_viewing_key(&mut app, query_auth.clone(), mock_info("violet", &[]));
//     let viewing_key_magenta =
//         create_viewing_key(&mut app, query_auth.clone(), mock_info("magenta", &[]));
//     let viewing_key_gold = create_viewing_key(&mut app, query_auth.clone(), mock_info("gold", &[]));
//     let viewing_key_crimson =
//         create_viewing_key(&mut app, query_auth.clone(), mock_info("crimson", &[]));
//     let viewing_key_turquoise =
//         create_viewing_key(&mut app, query_auth.clone(), mock_info("turquoise", &[]));

//     suite
//         .vote(
//             "blue",
//             Auth::ViewingKey {
//                 key: viewing_key_blue,
//                 address: "blue".to_string(),
//             },
//             1,
//             vec![0, 2, 1],
//         )
//         .unwrap();
//     suite
//         .vote(
//             "violet",
//             Auth::ViewingKey {
//                 key: viewing_key_violet,
//                 address: "violet".to_string(),
//             },
//             1,
//             vec![1, 0, 2],
//         )
//         .unwrap();
//     suite
//         .vote(
//             "magenta",
//             Auth::ViewingKey {
//                 key: viewing_key_magenta,
//                 address: "magenta".to_string(),
//             },
//             1,
//             vec![2, 1, 0],
//         )
//         .unwrap();
//     suite
//         .vote(
//             "gold",
//             Auth::ViewingKey {
//                 key: viewing_key_gold,
//                 address: "gold".to_string(),
//             },
//             1,
//             vec![1, 0, 2],
//         )
//         .unwrap();
//     suite
//         .vote(
//             "crimson",
//             Auth::ViewingKey {
//                 key: viewing_key_crimson,
//                 address: "crimson".to_string(),
//             },
//             1,
//             vec![0, 2, 1],
//         )
//         .unwrap();
//     suite
//         .vote(
//             "turquoise",
//             Auth::ViewingKey {
//                 key: viewing_key_turquoise,
//                 address: "turquoise".to_string(),
//             },
//             1,
//             vec![2, 0, 1],
//         )
//         .unwrap();

//     suite.a_day_passes();

//     let (winner, status) = suite.query_winner_and_status(1);
//     assert_eq!(winner, Winner::Never);
//     assert_eq!(status, Status::Rejected);

//     suite.close("crimson", 1).unwrap();

//     let (_, status) = suite.query_winner_and_status(1);
//     assert_eq!(status, Status::Closed);
// }

#[test]
fn test_make_proposal_fails() {
    let mut suite = SuiteBuilder::default().build();
    let mut app = App::default();
    let query_auth = instantiate_query_auth(&mut app);
    let viewing_key_sender = create_viewing_key(
        &mut app,
        query_auth,
        mock_info(&suite.sender().to_string(), &[]),
    );
    suite
        .propose(
            suite.sender(),
            Auth::ViewingKey {
                key: viewing_key_sender,
                address: suite.sender().to_string(),
            },
            vec![vec![unimportant_message()]],
        )
        .unwrap_err();
}

#[test]
fn test_proposal_zero_choices() {
    let mut suite = SuiteBuilder::default().build();
    let mut app = App::default();
    let query_auth = instantiate_query_auth(&mut app);
    let viewing_key_sender = create_viewing_key(
        &mut app,
        query_auth,
        mock_info(&suite.sender().to_string(), &[]),
    );
    let err = suite.propose(
        suite.sender(),
        Auth::ViewingKey {
            key: viewing_key_sender,
            address: suite.sender().to_string(),
        },
        vec![],
    );
    is_error!(err, &ContractError::ZeroChoices {}.to_string());
}

#[test]
fn test_no_propose_zero_voting_power_fails() {
    let mut suite = SuiteBuilder::default().build();
    let mut app = App::default();
    let query_auth = instantiate_query_auth(&mut app);
    let viewing_key_someone = create_viewing_key(&mut app, query_auth, mock_info("someone", &[]));
    suite
        .propose(
            "someone",
            Auth::ViewingKey {
                key: viewing_key_someone,
                address: "someone".to_string(),
            },
            vec![],
        )
        .unwrap_err();
}
