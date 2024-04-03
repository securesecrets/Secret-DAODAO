#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Reply, Response,
    StdResult, SubMsg, WasmMsg,
};
use cosmwasm_std::{Addr, Coin};

use cw_denom::CheckedDenom;
use cw_vesting::msg::{
    InstantiateMsg as PayrollInstantiateMsg, QueryMsg as PayrollQueryMsg,
    ReceiveMsg as PayrollReceiveMsg,
};
use cw_vesting::vesting::Vest;
use secret_cw2::set_contract_version;
use secret_utils::{nonpayable, parse_reply_instantiate_data};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, ReceiveMsg, Snip20ReceiveMsg};
use crate::state::{
    vesting_contracts, VestingContract, VestingContractInstantiateInfo, TMP_INSTANTIATOR_INFO,
    VESTING_INFO,
};

pub(crate) const CONTRACT_NAME: &str = "crates.io:cw-payroll-factory";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const INSTANTIATE_CONTRACT_REPLY_ID: u64 = 0;
pub const DEFAULT_LIMIT: u32 = 10;
pub const MAX_LIMIT: u32 = 50;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    cw_ownable::initialize_owner(deps.storage, deps.api, msg.owner.as_deref())?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    VESTING_INFO.save(
        deps.storage,
        &VestingContractInstantiateInfo {
            code_id: msg.vesting_code_id,
            code_hash: msg.vesting_code_hash,
        },
    )?;
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
        ExecuteMsg::Receive(msg) => execute_receive_snip20(env, deps, info, msg),
        ExecuteMsg::InstantiateNativePayrollContract {
            instantiate_msg,
            label,
        } => execute_instantiate_native_payroll_contract(deps, info, instantiate_msg, label),
        ExecuteMsg::UpdateOwnership(action) => execute_update_owner(deps, info, env, action),
        ExecuteMsg::UpdateCodeIdAndCodeHash { vesting_code_id,vesting_code_hash } => {
            execute_update_code_id_and_code_hash(deps, info, vesting_code_id,vesting_code_hash)
        }
    }
}

pub fn execute_receive_snip20(
    _env: Env,
    deps: DepsMut,
    info: MessageInfo,
    receive_msg: Snip20ReceiveMsg,
) -> Result<Response, ContractError> {
    // Only accepts snip20 tokens
    nonpayable(&info)?;

    let msg: ReceiveMsg = from_binary(&receive_msg.msg.unwrap())?;

    if TMP_INSTANTIATOR_INFO.may_load(deps.storage)?.is_some() {
        return Err(ContractError::Reentrancy);
    }

    // Save instantiator info for use in reply (snip20 sender in this case)
    let sender = deps.api.addr_validate(&receive_msg.sender)?;
    TMP_INSTANTIATOR_INFO.save(deps.storage, &sender)?;

    match msg {
        ReceiveMsg::InstantiatePayrollContract {
            instantiate_msg,
            label,
        } => {
            if receive_msg.amount != instantiate_msg.total {
                return Err(ContractError::WrongFundAmount {
                    sent: receive_msg.amount,
                    expected: instantiate_msg.total,
                });
            }
            instantiate_contract(deps, sender, None, instantiate_msg, label)
        }
    }
}

pub fn execute_instantiate_native_payroll_contract(
    deps: DepsMut,
    info: MessageInfo,
    instantiate_msg: PayrollInstantiateMsg,
    label: String,
) -> Result<Response, ContractError> {
    // Save instantiator info for use in reply
    TMP_INSTANTIATOR_INFO.save(deps.storage, &info.sender)?;

    instantiate_contract(deps, info.sender, Some(info.funds), instantiate_msg, label)
}

/// `sender` here refers to the initiator of the vesting, not the
/// literal sender of the message. Practically speaking, this means
/// that it should be set to the sender of the snip20's being vested,
/// and not the snip20 contract when dealing with non-native vesting.
pub fn instantiate_contract(
    deps: DepsMut,
    sender: Addr,
    funds: Option<Vec<Coin>>,
    instantiate_msg: PayrollInstantiateMsg,
    label: String,
) -> Result<Response, ContractError> {
    // Check sender is contract owner if set
    let ownership = cw_ownable::get_ownership(deps.storage)?;
    if ownership
        .owner
        .as_ref()
        .map_or(false, |owner| *owner != sender)
    {
        return Err(ContractError::Unauthorized {});
    }

    let vesting_contract_info = VESTING_INFO.load(deps.storage)?;

    // Instantiate the specified contract with owner as the admin.
    let instantiate = WasmMsg::Instantiate {
        admin: instantiate_msg.owner.clone(),
        code_id: vesting_contract_info.code_id,
        code_hash: vesting_contract_info.code_hash,
        msg: to_binary(&instantiate_msg)?,
        funds: funds.unwrap_or_default(),
        label,
    };

    let msg = SubMsg::reply_on_success(instantiate, INSTANTIATE_CONTRACT_REPLY_ID);

    Ok(Response::default()
        .add_attribute("action", "instantiate_cw_vesting")
        .add_submessage(msg))
}

pub fn execute_update_owner(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    action: cw_ownable::Action,
) -> Result<Response, ContractError> {
    let ownership = cw_ownable::update_ownership(deps, &env.block, &info.sender, action)?;
    Ok(Response::default().add_attributes(ownership.into_attributes()))
}

pub fn execute_update_code_id_and_code_hash(
    deps: DepsMut,
    info: MessageInfo,
    code_id: u64,
    code_hash: String,
) -> Result<Response, ContractError> {
    cw_ownable::assert_owner(deps.storage, &info.sender)?;
    VESTING_INFO.save(deps.storage, &VestingContractInstantiateInfo{
        code_id,
        code_hash: code_hash.clone()
    })?;
    Ok(Response::default()
        .add_attribute("action", "update_code_id")
        .add_attribute("vesting_code_id", code_id.to_string())
    .add_attribute("vesting_code_hash", code_hash))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::ListVestingContracts { start_after, limit } => {
            let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
            let start = start_after.as_deref().map(Bound::exclusive);

            let res: Vec<VestingContract> = vesting_contracts()
                .range(deps.storage, start, None, Order::Ascending)
                .take(limit)
                .flat_map(|vc| Ok::<VestingContract, ContractError>(vc?.1))
                .collect();

            Ok(to_binary(&res)?)
        }
        QueryMsg::ListVestingContractsReverse {
            start_before,
            limit,
        } => {
            let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
            let start = start_before.as_deref().map(Bound::exclusive);

            let res: Vec<VestingContract> = vesting_contracts()
                .range(deps.storage, None, start, Order::Descending)
                .take(limit)
                .flat_map(|vc| Ok::<VestingContract, ContractError>(vc?.1))
                .collect();

            Ok(to_binary(&res)?)
        }
        QueryMsg::ListVestingContractsByInstantiator {
            instantiator,
            start_after,
            limit,
        } => {
            let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
            let start = start_after.map(Bound::<String>::exclusive);

            // Validate owner address
            deps.api.addr_validate(&instantiator)?;

            let res: Vec<VestingContract> = vesting_contracts()
                .idx
                .instantiator
                .prefix(instantiator)
                .range(deps.storage, start, None, Order::Ascending)
                .take(limit)
                .flat_map(|vc| Ok::<VestingContract, ContractError>(vc?.1))
                .collect();

            Ok(to_binary(&res)?)
        }
        QueryMsg::ListVestingContractsByInstantiatorReverse {
            instantiator,
            start_before,
            limit,
        } => {
            let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
            let start = start_before.map(Bound::<String>::exclusive);

            // Validate owner address
            deps.api.addr_validate(&instantiator)?;

            let res: Vec<VestingContract> = vesting_contracts()
                .idx
                .instantiator
                .prefix(instantiator)
                .range(deps.storage, None, start, Order::Descending)
                .take(limit)
                .flat_map(|vc| Ok::<VestingContract, ContractError>(vc?.1))
                .collect();

            Ok(to_binary(&res)?)
        }
        QueryMsg::ListVestingContractsByRecipient {
            recipient,
            start_after,
            limit,
        } => {
            let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
            let start = start_after.map(Bound::<String>::exclusive);

            // Validate recipient address
            deps.api.addr_validate(&recipient)?;

            let res: Vec<VestingContract> = vesting_contracts()
                .idx
                .recipient
                .prefix(recipient)
                .range(deps.storage, start, None, Order::Ascending)
                .take(limit)
                .flat_map(|vc| Ok::<VestingContract, ContractError>(vc?.1))
                .collect();

            Ok(to_binary(&res)?)
        }
        QueryMsg::ListVestingContractsByRecipientReverse {
            recipient,
            start_before,
            limit,
        } => {
            let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
            let start = start_before.map(Bound::<String>::exclusive);

            // Validate recipient address
            deps.api.addr_validate(&recipient)?;

            let res: Vec<VestingContract> = vesting_contracts()
                .idx
                .recipient
                .prefix(recipient)
                .range(deps.storage, None, start, Order::Descending)
                .take(limit)
                .flat_map(|vc| Ok::<VestingContract, ContractError>(vc?.1))
                .collect();

            Ok(to_binary(&res)?)
        }
        QueryMsg::Ownership {} => to_binary(&cw_ownable::get_ownership(deps.storage)?),
        QueryMsg::CodeIdAndHash {} => to_binary(&VESTING_INFO.load(deps.storage)?),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_CONTRACT_REPLY_ID => {
            let res = parse_reply_instantiate_data(msg)?;
            let contract_addr = deps.api.addr_validate(&res.contract_address)?;

            // Query new vesting payment contract for info
            let vest: Vest = deps
                .querier
                .query_wasm_smart(contract_addr.clone(), &PayrollQueryMsg::Info {})?;

            let instantiator = TMP_INSTANTIATOR_INFO.load(deps.storage)?;

            // Save vesting contract payment info
            vesting_contracts().save(
                deps.storage,
                contract_addr.as_ref(),
                &VestingContract {
                    instantiator: instantiator.to_string(),
                    recipient: vest.recipient.to_string(),
                    contract: contract_addr.to_string(),
                },
            )?;

            // Clear tmp instatiator info
            TMP_INSTANTIATOR_INFO.remove(deps.storage);

            // If cw20, fire off fund message!
            let msgs: Vec<CosmosMsg> = match vest.denom {
                CheckedDenom::Native(_) => vec![],
                CheckedDenom::Cw20(ref denom, code_hash) => {
                    // Send transaction to fund contract
                    vec![CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: denom.to_string(),
                        code_hash,
                        msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Send {
                            recipient: contract_addr.to_string(),
                            recipient_code_hash: todo!(),
                            amount: vest.total(),
                            msg: Some(to_binary(&PayrollReceiveMsg::Fund {})?),
                            amount: todo!(),
                            memo: todo!(),
                            decoys: todo!(),
                            entropy: todo!(),
                            padding: todo!(),
                        })?,
                        funds: vec![],
                    })]
                }
            };

            Ok(Response::default()
                .add_attribute("new_payroll_contract", contract_addr)
                .add_messages(msgs))
        }
        _ => Err(ContractError::UnknownReplyId { id: msg.id }),
    }
}
