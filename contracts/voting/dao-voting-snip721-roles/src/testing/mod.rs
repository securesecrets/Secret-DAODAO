mod execute;
mod instantiate;
mod queries;
mod tests;

use cosmwasm_std::{Addr, ContractInfo, Empty};
use instantiate::instantiate_query_auth;
use secret_multi_test::{App, Contract, ContractWrapper, Executor};
use shade_protocol::utils::asset::RawContract;

use crate::msg::{InstantiateMsg, NftMintMsg, NftRolesContract};

use self::instantiate::instantiate_snip721_roles;

/// Address used as the owner, instantiator, and minter.
pub(crate) const CREATOR_ADDR: &str = "creator";

pub fn dao_voting_snip721_roles_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    )
    .with_reply(crate::contract::reply);
    Box::new(contract)
}

pub(crate) struct CommonTest {
    app: App,
    module_info: ContractInfo,
    query_auth: ContractInfo,
}

pub(crate) fn setup_test(initial_nfts: Vec<NftMintMsg>) -> CommonTest {
    let mut app = App::default();
    let module_id = app.store_code(dao_voting_snip721_roles_contract());
    let query_auth = instantiate_query_auth(&mut app);

    let (snip721_roles_info, snip721_roles_id, _) =
        instantiate_snip721_roles(&mut app, CREATOR_ADDR);
    let module_info = app
        .instantiate_contract(
            module_id,
            Addr::unchecked(CREATOR_ADDR),
            &InstantiateMsg {
                nft_contract: NftRolesContract::New {
                    snip721_roles_code_id: snip721_roles_id,
                    snip721_roles_code_hash: snip721_roles_info.code_hash,
                    name: "Job Titles".to_string(),
                    symbol: "TITLES".to_string(),
                    initial_nfts,
                    admin: None,
                    entropy: "entropy".into(),
                    royalty_info: None,
                    config: None,
                    post_init_callback: None,
                    query_auth: Some(RawContract::new(
                        &query_auth.address.clone().to_string(),
                        &query_auth.code_hash.clone(),
                    )),
                },
            },
            &[],
            "snip721_voting",
            None,
        )
        .unwrap();

    CommonTest {
        app,
        module_info,
        query_auth,
    }
}
