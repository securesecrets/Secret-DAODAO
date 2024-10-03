#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdResult, SubMsg,
    SubMsgResult, Uint128,
};
use dao_interface::replies::parse_reply_address_from_event;
use dao_interface::state::AnyContractInfo;
use secret_cw2::set_contract_version;
use snip20_reference_impl::msg::QueryAnswer;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, TokenInfo};
use crate::state::{DAO, TOKEN};
use secret_toolkit::utils::InitCallback;

const CONTRACT_NAME: &str = "crates.io:cw20-balance-voting";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const INSTANTIATE_TOKEN_REPLY_ID: u64 = 0;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    DAO.save(
        deps.storage,
        &AnyContractInfo {
            addr: info.sender.clone(),
            code_hash: msg.dao_code_hash,
        },
    )?;

    match msg.token_info {
        TokenInfo::Existing { address, code_hash } => {
            let address = deps.api.addr_validate(&address)?;
            TOKEN.save(
                deps.storage,
                &AnyContractInfo {
                    addr: address.clone(),
                    code_hash,
                },
            )?;
            Ok(Response::default()
                .add_attribute("action", "instantiate")
                .add_attribute("token", "existing_token")
                .add_attribute("token_address", address))
        }
        TokenInfo::New {
            code_id,
            code_hash,
            label,
            name,
            symbol,
            decimals,
            initial_balances,
        } => {
            let initial_supply = initial_balances
                .iter()
                .fold(Uint128::zero(), |p, n| p + n.amount);
            if initial_supply.is_zero() {
                return Err(ContractError::InitialBalancesError {});
            }

            let init_msg = snip20_reference_impl::msg::InstantiateMsg {
                name,
                symbol,
                decimals,
                initial_balances: Some(initial_balances),
                admin: None,
                prng_seed: to_binary("seed")?,
                config: None,
                supported_denoms: None,
            };
            let sub_msg = SubMsg::reply_on_success(
                init_msg.to_cosmos_msg(None, label, code_id, code_hash.clone(), None)?,
                INSTANTIATE_TOKEN_REPLY_ID,
            );

            TOKEN.save(
                deps.storage,
                &AnyContractInfo {
                    addr: Addr::unchecked(""),
                    code_hash,
                },
            )?;

            Ok(Response::default()
                .add_attribute("action", "instantiate")
                .add_attribute("token", "new_token")
                .add_submessage(sub_msg))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {}
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::TokenContract {} => query_token_contract(deps),
        QueryMsg::VotingPowerAtHeight { auth, height: _ } => {
            let mut viewing_key = String::new();
            let mut addr = String::new();
            match auth {
                shade_protocol::basic_staking::Auth::ViewingKey { key, address } => {
                    viewing_key = key;
                    addr = address;
                }
                shade_protocol::basic_staking::Auth::Permit(_) => (),
            };
            query_voting_power_at_height(deps, env, addr, viewing_key)
        }
        QueryMsg::TotalPowerAtHeight { height: _ } => query_total_power_at_height(deps, env),
        QueryMsg::Info {} => query_info(deps),
        QueryMsg::Dao {} => query_dao(deps),
    }
}

pub fn query_dao(deps: Deps) -> StdResult<Binary> {
    let dao = DAO.load(deps.storage)?;
    to_binary(&dao)
}

pub fn query_token_contract(deps: Deps) -> StdResult<Binary> {
    let token = TOKEN.load(deps.storage)?;
    to_binary(&token)
}

pub fn query_voting_power_at_height(
    deps: Deps,
    env: Env,
    address: String,
    key: String,
) -> StdResult<Binary> {
    let token = TOKEN.load(deps.storage)?;
    let address = deps.api.addr_validate(&address)?;
    let mut balance_amount = Uint128::zero();
    let balance: snip20_reference_impl::msg::QueryAnswer = deps.querier.query_wasm_smart(
        token.code_hash,
        token.addr,
        &snip20_reference_impl::msg::QueryMsg::Balance {
            address: address.to_string(),
            key,
        },
    )?;
    if let QueryAnswer::Balance { amount } = balance {
        balance_amount = amount
    }
    println!("balance : {}", balance_amount);
    to_binary(&dao_interface::voting::VotingPowerAtHeightResponse {
        power: balance_amount,
        height: env.block.height,
    })
}

pub fn query_total_power_at_height(deps: Deps, env: Env) -> StdResult<Binary> {
    let token = TOKEN.load(deps.storage)?;
    let mut supply = Uint128::zero();
    let info: snip20_reference_impl::msg::QueryAnswer = deps.querier.query_wasm_smart(
        token.code_hash,
        token.addr,
        &snip20_reference_impl::msg::QueryMsg::TokenInfo {},
    )?;
    if let QueryAnswer::TokenInfo { total_supply, .. } = info {
        supply = total_supply.unwrap_or_default();
    }
    to_binary(&dao_interface::voting::TotalPowerAtHeightResponse {
        power: supply,
        height: env.block.height,
    })
}

pub fn query_info(deps: Deps) -> StdResult<Binary> {
    let info = secret_cw2::get_contract_version(deps.storage)?;
    to_binary(&dao_interface::voting::InfoResponse { info })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_TOKEN_REPLY_ID => match msg.result {
            SubMsgResult::Ok(sub_msg_response) => {
                let mut token_info = TOKEN.load(deps.storage)?;
                if token_info.addr != Addr::unchecked("") {
                    return Err(ContractError::DuplicateToken {});
                }

                let token = parse_reply_address_from_event(sub_msg_response);
                token_info.addr = deps.api.addr_validate(&token)?;
                TOKEN.save(deps.storage, &token_info)?;
                Ok(Response::default().add_attribute("token_address", token))
            }
            SubMsgResult::Err(_) => Err(ContractError::TokenInstantiateError {}),
        },
        _ => Err(ContractError::UnknownReplyId { id: msg.id }),
    }
}
