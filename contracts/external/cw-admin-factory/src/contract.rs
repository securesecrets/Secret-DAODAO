#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError, StdResult, SubMsg,
    SubMsgResult, WasmMsg,
};
use dao_interface::replies::parse_reply_address_from_event;
use secret_cw2::set_contract_version;

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
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::InstantiateContractWithSelfAdmin {
            instantiate_msg,
            code_id,
            code_hash,
            label,
        } => instantiate_contract(deps, env, info, instantiate_msg, code_id, code_hash, label),
    }
}

pub fn instantiate_contract(
    _deps: DepsMut,
    env: Env,
    info: MessageInfo,
    instantiate_msg: Binary,
    code_id: u64,
    code_hash: String,
    label: String,
) -> Result<Response, ContractError> {
    // Instantiate the specified contract with factory as the admin.
    let instantiate = WasmMsg::Instantiate {
        admin: Some(env.contract.address.to_string()),
        code_id,
        msg: instantiate_msg,
        funds: info.funds,
        label,
        code_hash,
    };

    let msg = SubMsg::reply_on_success(instantiate, INSTANTIATE_CONTRACT_REPLY_ID);
    Ok(Response::default()
        .add_attribute("action", "instantiate_contract_with_self_admin")
        .add_submessage(msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {}
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(_deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_CONTRACT_REPLY_ID => match msg.result {
            cosmwasm_std::SubMsgResult::Ok(res) => {
                let address = parse_reply_address_from_event(res);

                // Make the contract its own admin.
                let msg = WasmMsg::UpdateAdmin {
                    contract_addr: address.clone(),
                    admin: address.clone(),
                };

                Ok(Response::default()
                    .add_attribute("set contract admin as itself", address)
                    .add_message(msg))
            }
            SubMsgResult::Err(err) => Err(ContractError::Std(StdError::GenericErr { msg: err })),
        },
        _ => Err(ContractError::UnknownReplyID {}),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    // Set contract to version to latest
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}
