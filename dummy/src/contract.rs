use crate::math;
use cosmwasm_std::{
    entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
    Uint128,Empty,from_binary,Addr
};
use dao_voting::duration::validate_duration;
use crate::msg::{CountResponse, ExecuteMsg, InstantiateMsg, QueryMsg,ReceiveMsg,Snip20ReceiveMsg};
use crate::state::{Config,BALANCE, CONFIG, STAKED_TOTAL,STAKED_BALANCES,MAX_CLAIMS,HOOKS,CLAIMS};
use secret_cw2::set_contract_version;
use crate::error::ContractError;
use dao_hooks::stake::{stake_hook_msgs, unstake_hook_msgs};
pub(crate) const CONTRACT_NAME: &str = "crates.io:snip20-stake";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<Empty>, ContractError> {
    cw_ownable::initialize_owner(deps.storage, deps.api, msg.owner.as_deref())?;
    let token_address = deps.api.addr_validate(&msg.token_address)?;

   validate_duration(msg.unstaking_duration)?;
    let config = Config {
        token_address,
        unstaking_duration: msg.unstaking_duration,
    };
    CONFIG.save(deps.storage, &config).unwrap();
    // Initialize state to zero. We do this instead of using
    // `unwrap_or_default` where this is used as it protects us
    // against a scenerio where state is cleared by a bad actor and
    // `unwrap_or_default` carries on.

    STAKED_TOTAL.save(deps.storage, &Uint128::zero())?;
    BALANCE.save(deps.storage, &Uint128::zero())?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default())
}

#[entry_point]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response<Empty>, ContractError> {
    match msg {
        // ExecuteMsg::Receive(msg) => execute_receive(deps, env, info, msg),
        ExecuteMsg::Unstake { amount } => execute_unstake(deps, env, info, amount),
        ExecuteMsg::UpdateOwnership(action) => unimplemented!(),
    }
}



// pub fn execute_receive(
//     deps: DepsMut,
//     env: Env,
//     info: MessageInfo,
//     wrapper: Snip20ReceiveMsg,
// ) -> Result<Response, ContractError> {
//     let config = CONFIG.load(deps.storage)?;
//     if info.sender != config.token_address {
//         return Err(ContractError::InvalidToken {
//             received: info.sender,
//             expected: config.token_address,
//         });
//     }
//     let msg: ReceiveMsg = from_binary(&wrapper.msg)?;
//     let sender = deps.api.addr_validate(&wrapper.sender)?;
//     match msg {
//         ReceiveMsg::Stake {} => execute_stake(deps, env, sender, wrapper.amount),
//         ReceiveMsg::Fund {} => execute_fund(deps, env, &sender, wrapper.amount),
//     }
// }

// pub fn execute_stake(
//     deps: DepsMut,
//     env: Env,
//     sender: Addr,
//     amount: Uint128,
// ) -> Result<Response, ContractError> {
//     let balance = BALANCE.load(deps.storage)?;
//     let staked_total = STAKED_TOTAL.load(deps.storage)?;
//     let amount_to_stake = math::amount_to_stake(staked_total, balance, amount);
//     let prev_balance = STAKED_BALANCES.get(deps.storage, &sender);
//     STAKED_BALANCES.insert(
//         deps.storage,
//         &sender,
//         &prev_balance
//             .unwrap_or_default()
//             .checked_add(amount_to_stake)
//             .unwrap(),
//     )?;
//     STAKED_TOTAL.update(deps.storage, |total| -> StdResult<Uint128> {
//         // Initialized during instantiate - OK to unwrap.
//         Ok(total.checked_add(amount_to_stake)?)
//     })?;
//     BALANCE.save(
//         deps.storage,
//         &balance.checked_add(amount).map_err(StdError::overflow)?,
//     )?;
//     let hook_msgs = stake_hook_msgs(
//         HOOKS,
//         deps.storage,
//         sender.clone(),
//         amount_to_stake,
//         env.contract.code_hash,
//     )?;
//     Ok(Response::new()
//         .add_submessages(hook_msgs)
//         .add_attribute("action", "stake")
//         .add_attribute("from", sender)
//         .add_attribute("amount", amount))
// }

// pub fn execute_fund(
//     deps: DepsMut,
//     _env: Env,
//     sender: &Addr,
//     amount: Uint128,
// ) -> Result<Response, ContractError> {
//     BALANCE.update(deps.storage, |balance| -> StdResult<_> {
//         balance.checked_add(amount).map_err(StdError::overflow)
//     })?;
//     Ok(Response::new()
//         .add_attribute("action", "fund")
//         .add_attribute("from", sender)
//         .add_attribute("amount", amount))
// }

pub fn execute_unstake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let balance = BALANCE.load(deps.storage)?;
    let staked_total = STAKED_TOTAL.load(deps.storage)?;
    // invariant checks for amount_to_claim
    if staked_total.is_zero() {
        return Err(ContractError::NothingStaked {});
    }
    if amount.checked_add(balance).unwrap() == Uint128::MAX {
        return Err(ContractError::Cw20InvaraintViolation {});
    }
    if amount > staked_total {
        return Err(ContractError::ImpossibleUnstake {});
    }
    let amount_to_claim = math::amount_to_claim(staked_total, balance, amount);
    let mut prev_balance = STAKED_BALANCES.load(deps.storage, info.sender.clone())?;
    prev_balance=prev_balance-amount;

    STAKED_BALANCES.save(
        deps.storage,
        info.sender.clone(),
        &prev_balance
    )?;
    STAKED_TOTAL.update(deps.storage, |total| -> StdResult<Uint128> {
        // Initialized during instantiate - OK to unwrap.
        Ok(total.checked_sub(amount)?)
    })?;
    BALANCE.save(
        deps.storage,
        &balance
            .checked_sub(amount_to_claim)
            .map_err(StdError::overflow)?,
    )?;
    let hook_msgs = unstake_hook_msgs(
        HOOKS,
        deps.storage,
        info.sender.clone(),
        amount,
        env.contract.code_hash.clone(),
    )?;
    match config.unstaking_duration {
        None => {
            let cw_send_msg = secret_toolkit::snip20::HandleMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: amount_to_claim,
                memo: None,
                padding: None,
            };
            let wasm_msg = cosmwasm_std::WasmMsg::Execute {
                contract_addr: config.token_address.to_string(),
                code_hash: env.contract.code_hash,
                msg: to_binary(&cw_send_msg)?,
                funds: vec![],
            };
            Ok(Response::new()
                .add_message(wasm_msg)
                .add_submessages(hook_msgs)
                .add_attribute("action", "unstake")
                .add_attribute("from", info.sender)
                .add_attribute("amount", amount)
                .add_attribute("claim_duration", "None"))
        }
        Some(duration) => {
            let outstanding_claims = CLAIMS.query_claims(deps.as_ref(), &info.sender)?.claims;
            if outstanding_claims.len() + 1 > MAX_CLAIMS as usize {
                return Err(ContractError::TooManyClaims {});
            }

            CLAIMS.create_claim(
                deps.storage,
                &info.sender,
                amount_to_claim,
                duration.after(&env.block),
            )?;
            Ok(Response::new()
                .add_attribute("action", "unstake")
                .add_submessages(hook_msgs)
                .add_attribute("from", info.sender)
                .add_attribute("amount", amount)
                .add_attribute("claim_duration", format!("{duration}")))
        }
    }
}

// pub fn execute_update_owner(
//     deps: DepsMut,
//     info: MessageInfo,
//     env: Env,
//     action: cw_ownable::Action,
// ) -> Result<Response, ContractError> {
//     let ownership = cw_ownable::update_ownership(deps, &env.block, &info.sender, action)?;
//     Ok(Response::default().add_attributes(ownership.into_attributes()))
// }


#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetCount {} => unimplemented!(),
    }
}

// fn query_count(deps: Deps) -> StdResult<CountResponse> {
//     let state = config_read(deps.storage).load()?;
//     Ok(CountResponse { count: state.count })
// }

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use cosmwasm_std::testing::*;
//     use cosmwasm_std::{from_binary, Coin, StdError, Uint128};

//     #[test]
//     fn proper_initialization() {
//         let mut deps = mock_dependencies();
//         let info = mock_info(
//             "creator",
//             &[Coin {
//                 denom: "earth".to_string(),
//                 amount: Uint128::new(1000),
//             }],
//         );
//         let init_msg = InstantiateMsg { count: 17 };

//         // we can just call .unwrap() to assert this was a success
//         let res = instantiate(deps.as_mut(), mock_env(), info, init_msg).unwrap();

//         assert_eq!(0, res.messages.len());

//         // it worked, let's query the state
//         let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCount {}).unwrap();
//         let value: CountResponse = from_binary(&res).unwrap();
//         assert_eq!(17, value.count);
//     }

//     #[test]
//     fn increment() {
//         let mut deps = mock_dependencies_with_balance(&[Coin {
//             denom: "token".to_string(),
//             amount: Uint128::new(2),
//         }]);
//         let info = mock_info(
//             "creator",
//             &[Coin {
//                 denom: "token".to_string(),
//                 amount: Uint128::new(2),
//             }],
//         );
//         let init_msg = InstantiateMsg { count: 17 };

//         let _res = instantiate(deps.as_mut(), mock_env(), info, init_msg).unwrap();

//         // anyone can increment
//         let info = mock_info(
//             "anyone",
//             &[Coin {
//                 denom: "token".to_string(),
//                 amount: Uint128::new(2),
//             }],
//         );

//         let exec_msg = ExecuteMsg::Increment {};
//         let _res = execute(deps.as_mut(), mock_env(), info, exec_msg).unwrap();

//         // should increase counter by 1
//         let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCount {}).unwrap();
//         let value: CountResponse = from_binary(&res).unwrap();
//         assert_eq!(18, value.count);
//     }

//     #[test]
//     fn reset() {
//         let mut deps = mock_dependencies_with_balance(&[Coin {
//             denom: "token".to_string(),
//             amount: Uint128::new(2),
//         }]);
//         let info = mock_info(
//             "creator",
//             &[Coin {
//                 denom: "token".to_string(),
//                 amount: Uint128::new(2),
//             }],
//         );
//         let init_msg = InstantiateMsg { count: 17 };

//         let _res = instantiate(deps.as_mut(), mock_env(), info, init_msg).unwrap();

//         // not anyone can reset
//         let info = mock_info(
//             "anyone",
//             &[Coin {
//                 denom: "token".to_string(),
//                 amount: Uint128::new(2),
//             }],
//         );
//         let exec_msg = ExecuteMsg::Reset { count: 5 };

//         let res = execute(deps.as_mut(), mock_env(), info, exec_msg);

//         match res {
//             Err(StdError::GenericErr { .. }) => {}
//             _ => panic!("Must return unauthorized error"),
//         }

//         // only the original creator can reset the counter
//         let info = mock_info(
//             "creator",
//             &[Coin {
//                 denom: "token".to_string(),
//                 amount: Uint128::new(2),
//             }],
//         );
//         let exec_msg = ExecuteMsg::Reset { count: 5 };

//         let _res = execute(deps.as_mut(), mock_env(), info, exec_msg).unwrap();

//         // should now be 5
//         let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCount {}).unwrap();
//         let value: CountResponse = from_binary(&res).unwrap();
//         assert_eq!(5, value.count);
//     }
// }
