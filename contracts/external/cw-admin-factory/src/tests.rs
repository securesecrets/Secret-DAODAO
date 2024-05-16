// use std::vec;

// use cosmwasm_std::{
//     testing::{mock_dependencies, mock_env, mock_info},
//     to_binary, Addr, Binary, Empty, Reply, SubMsg, SubMsgResponse, SubMsgResult, WasmMsg,
// };

// use secret_multi_test::{App, AppResponse, Contract, ContractWrapper, Executor};
// use dao_interface::state::{Admin, ModuleInstantiateInfo};

// use crate::{
//     contract::instantiate,
//     contract::{migrate, reply, CONTRACT_NAME, CONTRACT_VERSION, INSTANTIATE_CONTRACT_REPLY_ID},
//     msg::{ExecuteMsg, InstantiateMsg, MigrateMsg},
// };

// fn factory_contract() -> Box<dyn Contract<Empty>> {
//     let contract = ContractWrapper::new(
//         crate::contract::execute,
//         crate::contract::instantiate,
//         crate::contract::query,
//     )
//     .with_reply(crate::contract::reply);
//     Box::new(contract)
// }

// fn snip20_contract() -> Box<dyn Contract<Empty>> {
//     let contract = ContractWrapper::new(
//         snip20_reference_impl::contract::execute,
//         snip20_reference_impl::contract::instantiate,
//         snip20_reference_impl::contract::query,
//     );
//     Box::new(contract)
// }

// fn cw_core_contract() -> Box<dyn Contract<Empty>> {
//     let contract = ContractWrapper::new(
//         dao_dao_core::contract::execute,
//         dao_dao_core::contract::instantiate,
//         dao_dao_core::contract::query,
//     )
//     .with_reply(dao_dao_core::contract::reply)
//     .with_migrate(dao_dao_core::contract::migrate);
//     Box::new(contract)
// }

// #[test]
// pub fn test_set_admin() {
//     let mut app = App::default();
//     let info = app.store_code(factory_contract());
//     let snip20_info = app.store_code(snip20_contract());
//     let cw20_instantiate = snip20_reference_impl::msg::InstantiateMsg {
//         name: "DAO".to_string(),
//         symbol: "DAO".to_string(),
//         decimals: 6,
//         initial_balances: Some(vec![]),
//         admin: None,
//         prng_seed: to_binary(&"seed".to_string()).unwrap(),
//         config: None,
//         supported_denoms: None,
//     };

//     let instantiate = InstantiateMsg {};
//     let factory_info = app
//         .instantiate_contract(
//             info,
//             Addr::unchecked("CREATOR"),
//             &instantiate,
//             &[],
//             "cw-admin-factory",
//             None,
//         )
//         .unwrap();

//     // Instantiate core contract using factory.
//     let cw_core_info = app.store_code(cw_core_contract());
//     let instantiate_core = dao_interface::msg::InstantiateMsg {
//         dao_uri: None,
//         admin: None,
//         name: "DAO DAO".to_string(),
//         description: "A DAO that builds DAOs.".to_string(),
//         image_url: None,
//         automatically_add_snip20s: true,
//         automatically_add_snip721s: true,
//         voting_module_instantiate_info: ModuleInstantiateInfo {
//             code_id: snip20_info.clone().code_id,
//             code_hash: snip20_info.clone().code_hash,
//             msg: to_binary(&cw20_instantiate).unwrap(),
//             admin: Some(Admin::CoreModule {}),
//             funds: vec![],
//             label: "voting module".to_string(),
//         },
//         proposal_modules_instantiate_info: vec![
//             ModuleInstantiateInfo {
//                 code_id: snip20_info.clone().code_id,
//                 code_hash: snip20_info.clone().code_hash,
//                 msg: to_binary(&cw20_instantiate).unwrap(),
//                 admin: Some(Admin::CoreModule {}),
//                 funds: vec![],
//                 label: "prop module".to_string(),
//             },
//             ModuleInstantiateInfo {
//                 code_id: snip20_info.code_id,
//                 code_hash: snip20_info.code_hash,
//                 msg: to_binary(&cw20_instantiate).unwrap(),
//                 admin: Some(Admin::CoreModule {}),
//                 funds: vec![],
//                 label: "prop module 2".to_string(),
//             },
//         ],
//         initial_items: None,
//         snip20_code_hash: "".to_string(),
//         snip721_code_hash: "".to_string()
//     };

//     let res: AppResponse = app
//         .execute_contract(
//             Addr::unchecked("CREATOR"),
//             &factory_info,
//             &ExecuteMsg::InstantiateContractWithSelfAdmin {
//                 module_info: ModuleInstantiateInfo{
//                     code_id: cw_core_info.code_id,
//                     code_hash: cw_core_info.code_hash,
//                     msg: to_binary(&instantiate_core).unwrap(),
//                     admin:None,
//                     funds: vec![],
//                     label:"label".to_string()
//                 }
//             },
//             &[],
//         )
//         .unwrap();

//     // // Get the core address from the instantiate event
//     // let instantiate_event = &res.events[2];
//     // assert_eq!(instantiate_event.ty, "instantiate");
//     // let core_addr = instantiate_event.attributes[0].value.clone();

//     // // Check that admin of core address is itself
//     // let contract_info = app.wrap().query_wasm_smart(&core_addr).unwrap();
//     // assert_eq!(contract_info.admin, Some(core_addr))
// }

// #[test]
// pub fn test_set_admin_mock() {
//     let mut deps = mock_dependencies();
//     // Instantiate factory contract
//     let instantiate_msg = InstantiateMsg {};
//     let info = mock_info("creator", &[]);
//     let env = mock_env();
//     instantiate(deps.as_mut(), env.clone(), info, instantiate_msg).unwrap();
//     let bytes = vec![10, 9, 99, 111, 110, 116, 114, 97, 99, 116, 50];
//     let reply_msg: Reply = Reply {
//         id: INSTANTIATE_CONTRACT_REPLY_ID,
//         result: SubMsgResult::Ok(SubMsgResponse {
//             events: vec![],
//             data: (Some(Binary(bytes))),
//         }),
//     };

//     let res = reply(deps.as_mut(), env, reply_msg).unwrap();
//     assert_eq!(res.attributes.len(), 1);
//     assert_eq!(
//         res.messages[0],
//         SubMsg::new(WasmMsg::UpdateAdmin {
//             contract_addr: "contract2".to_string(),
//             admin: "contract2".to_string()
//         })
//     )
// }

// #[test]
// pub fn test_migrate_update_version() {
//     let mut deps = mock_dependencies();
//     secret_cw2::set_contract_version(&mut deps.storage, "my-contract", "old-version").unwrap();
//     migrate(deps.as_mut(), mock_env(), MigrateMsg {}).unwrap();
//     let version = secret_cw2::get_contract_version(&deps.storage).unwrap();
//     assert_eq!(version.version, CONTRACT_VERSION);
//     assert_eq!(version.contract, CONTRACT_NAME);
// }
