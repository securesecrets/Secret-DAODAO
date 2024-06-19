mod adversarial;
mod execute;
mod hooks;
mod instantiate;
mod queries;
mod tests;

// Integrationg tests using an actual chain binary, requires
// the "test-tube" feature to be enabled
// cargo test --features test-tube
#[cfg(test)]
#[cfg(feature = "test-tube")]
mod integration_tests;
#[cfg(test)]
#[cfg(feature = "test-tube")]
mod test_tube_env;

use cosmwasm_std::{Addr, ContractInfo};
use instantiate::{
    instantiate_query_auth, instantiate_snip721_base, voting_snip721_staked_contract,
};
use secret_multi_test::{App, Executor};
use secret_utils::Duration;
use shade_protocol::utils::asset::RawContract;

use crate::msg::{InstantiateMsg, NftContract};

/// Address used as the owner, instantiator, and minter.
pub(crate) const CREATOR_ADDR: &str = "creator";

pub(crate) struct CommonTest {
    app: App,
    module: ContractInfo,
    nft: ContractInfo,
    query_auth: ContractInfo,
}

pub(crate) fn setup_test(unstaking_duration: Option<Duration>) -> CommonTest {
    let mut app = App::default();
    let module_id = app.store_code(voting_snip721_staked_contract());

    let nft = instantiate_snip721_base(&mut app, CREATOR_ADDR, CREATOR_ADDR);
    let query_auth = instantiate_query_auth(&mut app, CREATOR_ADDR);
    let module = app
        .instantiate_contract(
            module_id,
            Addr::unchecked(CREATOR_ADDR),
            &InstantiateMsg {
                nft_contract: NftContract::Existing {
                    address: nft.address.to_string(),
                    code_hash: nft.code_hash.clone(),
                },
                unstaking_duration,
                active_threshold: None,
                dao_code_hash: "dao_code_hash".to_string(),
                query_auth: Some(RawContract {
                    address: query_auth.address.to_string(),
                    code_hash: query_auth.code_hash.clone(),
                }),
            },
            &[],
            "snip721_voting",
            None,
        )
        .unwrap();
    CommonTest {
        app,
        module,
        nft,
        query_auth,
    }
}

// Advantage to using a macro for this is that the error trace links
// to the exact line that the error occured, instead of inside of a
// function where the assertion would otherwise happen.
macro_rules! is_error {
    ($x:expr => $e:tt) => {
        assert!(format!("{:#}", $x.unwrap_err()).contains($e))
    };
}

pub(crate) use is_error;
