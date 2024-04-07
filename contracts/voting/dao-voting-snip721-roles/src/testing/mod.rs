mod execute;
mod instantiate;
mod queries;
mod tests;

use cosmwasm_std::{Addr, ContractInfo, Empty};
use secret_multi_test::{App, Contract, ContractWrapper, Executor};

use crate::msg::{InstantiateMsg, NftContract, NftMintMsg};

use self::instantiate::{instantiate_snip721_roles, snip721_contract};

/// Address used as the owner, instantiator, and minter.
pub(crate) const CREATOR_ADDR: &str = "creator";

pub(crate) struct CommonTest {
    app: App,
    module_info: ContractInfo,
}

fn dao_voting_snip721_roles_contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );
    Box::new(contract)
}

pub(crate) fn setup_test() -> CommonTest {
    let mut app = App::default();
    let module_info = app.store_code(dao_voting_snip721_roles_contract());

    let snip721_roles_info =
        instantiate_snip721_roles(&mut app, CREATOR_ADDR);
    let module_info = app
        .instantiate_contract(
            module_info,
            Addr::unchecked(CREATOR_ADDR),
            &InstantiateMsg {
                nft_contract: NftContract::Existing { address: snip721_roles_info.address.to_string(), code_hash: snip721_roles_info.code_hash },
                dao_code_hash: "dao_code_hash".to_string(),
            },
            &[],
            "snip721_voting",
            None,
        )
        .unwrap();

    CommonTest { app, module_info }
}
