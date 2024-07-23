use crate::msg::{AdminListResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use anyhow::{anyhow, Result};
use assert_matches::assert_matches;
use cosmwasm_std::{
    to_binary, Addr, ContractInfo, CosmosMsg, Empty, QueryRequest, StdError, WasmMsg, WasmQuery,
};
use cw1::Cw1Contract;
use derivative::Derivative;
use secret_multi_test::{
    App, AppResponse, Contract, ContractInstantiationInfo, ContractWrapper, Executor,
};
use serde::{de::DeserializeOwned, Serialize};

fn mock_app() -> App {
    App::default()
}

fn contract_cw1() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );
    Box::new(contract)
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Suite {
    /// Application mock
    #[derivative(Debug = "ignore")]
    app: App,
    /// Special account
    pub owner: String,
    /// ID of stored code for cw1 contract
    cw1_contract_instantiate_info: ContractInstantiationInfo,
}

impl Suite {
    pub fn init() -> Result<Suite> {
        let mut app = mock_app();
        let owner = Addr::unchecked("owner").to_string();
        let cw1_contract_instantiate_info = app.store_code(contract_cw1());

        Ok(Suite {
            app,
            owner,
            cw1_contract_instantiate_info,
        })
    }

    pub fn instantiate_cw1_contract(&mut self, admins: Vec<String>, mutable: bool) -> Cw1Contract {
        let contract = self
            .app
            .instantiate_contract(
                self.cw1_contract_instantiate_info.clone(),
                Addr::unchecked(self.owner.clone()),
                &InstantiateMsg { admins, mutable },
                &[],
                "Whitelist",
                None,
            )
            .unwrap();
        Cw1Contract(contract.address, contract.code_hash)
    }

    pub fn execute<M>(
        &mut self,
        sender_contract_info: ContractInfo,
        target_contract: &Addr,
        target_contract_code_hash: String,
        msg: M,
    ) -> Result<AppResponse>
    where
        M: Serialize + DeserializeOwned,
    {
        let execute: ExecuteMsg = ExecuteMsg::Execute {
            msgs: vec![CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: target_contract.to_string(),
                code_hash: target_contract_code_hash,
                msg: to_binary(&msg)?,
                funds: vec![],
            })],
        };
        self.app
            .execute_contract(
                Addr::unchecked(self.owner.clone()),
                &sender_contract_info,
                &execute,
                &[],
            )
            .map_err(|err| anyhow!(err))
    }

    pub fn query<M>(
        &self,
        target_contract: Addr,
        target_contract_code_hash: String,
        msg: M,
    ) -> Result<AdminListResponse, StdError>
    where
        M: Serialize + DeserializeOwned,
    {
        self.app.wrap().query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: target_contract.to_string(),
            code_hash: target_contract_code_hash,
            msg: to_binary(&msg).unwrap(),
        }))
    }
}

#[test]
fn proxy_freeze_message() {
    let mut suite = Suite::init().unwrap();

    let first_contract = suite.instantiate_cw1_contract(vec![suite.owner.clone()], true);
    let second_contract =
        suite.instantiate_cw1_contract(vec![first_contract.addr().to_string()], true);
    assert_ne!(second_contract, first_contract);

    let freeze_msg: ExecuteMsg = ExecuteMsg::Freeze {};
    assert_matches!(
        suite.execute(
            ContractInfo {
                address: first_contract.addr(),
                code_hash: first_contract.code_hash()
            },
            &second_contract.addr(),
            second_contract.code_hash(),
            freeze_msg
        ),
        Ok(_)
    );

    let query_msg: QueryMsg = QueryMsg::AdminList {};
    assert_matches!(
        suite.query(second_contract.addr(), second_contract.code_hash(),query_msg),
        Ok(
            AdminListResponse {
                mutable,
                ..
            }) if !mutable
    );
}
