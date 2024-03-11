#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError, StdResult, SubMsg, WasmMsg,SubMsgResult
};
use dao_interface::state::ModuleInstantiateInfo;
use secret_cw2::set_contract_version;
use secret_utils::parse_reply_event_for_contract_address;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};

pub(crate) const CONTRACT_NAME: &str = "crates.io:cw-admin-factory";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const INSTANTIATE_CONTRACT_REPLY_ID: u64 = 0;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("creator", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    _deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::InstantiateContractWithSelfAdmin { module_info } => {
            instantiate_contract(env, info, module_info)
        }
    }
}

pub fn instantiate_contract(
    env: Env,
    _info: MessageInfo,
    module_info: ModuleInstantiateInfo,
) -> Result<Response, ContractError> {
    // Instantiate the specified contract with factory as the admin.
    let msg = module_info.to_cosmos_msg(env.contract.address);

    let msg = SubMsg::reply_on_success(msg, INSTANTIATE_CONTRACT_REPLY_ID);
    Ok(Response::default()
        .add_attribute("action", "instantiate_cw_core")
        .add_submessage(msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {}
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_CONTRACT_REPLY_ID => match msg.result {
            cosmwasm_std::SubMsgResult::Ok(res) => {
                let address = parse_reply_event_for_contract_address(res.events)?;
                let contract_addr = deps.api.addr_validate(&address)?;
                // Make the contract its own admin.
                let msg = WasmMsg::UpdateAdmin {
                    contract_addr: contract_addr.to_string(),
                    admin: contract_addr.to_string(),
                };

                Ok(Response::default()
                    .add_attribute("set contract admin as itself", contract_addr)
                    .add_message(msg))
            }
            SubMsgResult::Err(err) => Err(ContractError::Std(StdError::GenericErr { msg: err })),
        },
        // {
        //     let res = parse_reply_instantiate_data(msg)?;
        //     let contract_addr = deps.api.addr_validate(&res.contract_address)?;
        //     // Make the contract its own admin.
        //     let msg = WasmMsg::UpdateAdmin {
        //         contract_addr: contract_addr.to_string(),
        //         admin: contract_addr.to_string(),
        //     };

        //     Ok(Response::default()
        //         .add_attribute("set contract admin as itself", contract_addr)
        //         .add_message(msg))
        // }
        _ => Err(ContractError::UnknownReplyID {}),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    // Set contract to version to latest
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}
