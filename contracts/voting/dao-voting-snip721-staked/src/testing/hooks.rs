use cosmwasm_std::{
    testing::{mock_dependencies, mock_env, mock_info},
    Addr,
};
use dao_hooks::nft_stake::{stake_nft_hook_msgs, unstake_nft_hook_msgs};
use dao_interface::state::AnyContractInfo;
use shade_protocol::Contract;

use crate::{
    contract::execute,
    state::{Config, CONFIG, DAO, HOOKS},
};

// Shorthand for an unchecked address.
macro_rules! addr {
    ($x:expr ) => {
        Addr::unchecked($x)
    };
}

#[test]
fn test_hooks() {
    let mut deps = mock_dependencies();

    let messages = stake_nft_hook_msgs(
        HOOKS,
        &deps.storage,
        Addr::unchecked("ekez"),
        "ekez-token".to_string(),
    )
    .unwrap();
    assert_eq!(messages.len(), 0);

    let messages = unstake_nft_hook_msgs(
        HOOKS,
        &deps.storage,
        Addr::unchecked("ekez"),
        vec!["ekez-token".to_string()],
    )
    .unwrap();
    assert_eq!(messages.len(), 0);

    // Save a DAO address for the execute messages we're testing.
    DAO.save(
        deps.as_mut().storage,
        &AnyContractInfo {
            addr: addr!("ekez"),
            code_hash: "dummy_code_hash".to_string(),
        },
    )
    .unwrap();

    // Save a config for the execute messages we're testing.
    CONFIG
        .save(
            deps.as_mut().storage,
            &Config {
                nft_address: Addr::unchecked("ekez-token"),
                nft_code_hash: "nft_code_hash".to_string(),
                unstaking_duration: None,
                query_auth: Contract {
                    address: addr!("erty"),
                    code_hash: "dfgh".to_string(),
                },
            },
        )
        .unwrap();

    let env = mock_env();
    let info = mock_info("ekez", &[]);

    execute(
        deps.as_mut(),
        env,
        info,
        crate::msg::ExecuteMsg::AddHook {
            addr: "ekez".to_string(),
            code_hash: "efgh".to_string(),
        },
    )
    .unwrap();

    let messages = stake_nft_hook_msgs(
        HOOKS,
        &deps.storage,
        Addr::unchecked("ekez"),
        "ekez-token".to_string(),
    )
    .unwrap();
    assert_eq!(messages.len(), 1);

    let messages = unstake_nft_hook_msgs(
        HOOKS,
        &deps.storage,
        Addr::unchecked("ekez"),
        vec!["ekez-token".to_string()],
    )
    .unwrap();
    assert_eq!(messages.len(), 1);

    let env = mock_env();
    let info = mock_info("ekez", &[]);

    execute(
        deps.as_mut(),
        env,
        info,
        crate::msg::ExecuteMsg::RemoveHook {
            addr: "ekez".to_string(),
            code_hash: "efgh".to_string(),
        },
    )
    .unwrap();

    let messages = stake_nft_hook_msgs(
        HOOKS,
        &deps.storage,
        Addr::unchecked("ekez"),
        "ekez-token".to_string(),
    )
    .unwrap();
    assert_eq!(messages.len(), 0);

    let messages = unstake_nft_hook_msgs(
        HOOKS,
        &deps.storage,
        Addr::unchecked("ekez"),
        vec!["ekez-token".to_string()],
    )
    .unwrap();
    assert_eq!(messages.len(), 0);
}
