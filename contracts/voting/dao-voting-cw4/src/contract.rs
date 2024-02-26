use cosmwasm_schema::cw_serde;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdResult,
    SubMsg, SubMsgResult, Uint128,
};
use cw4::{MemberListResponse, MemberResponse, TotalWeightResponse};
use cw4_group::msg::InstantiateMsgResponse;
// use cw4_group::msg::InstantiateMsg as Cw4GroupInstantiateMsg;
use dao_interface::state::AnyContractInfo;
use secret_cw2::{get_contract_version, set_contract_version, ContractVersion};
use secret_toolkit::utils::InitCallback;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, GroupContract, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::{DAO, GROUP_CONTRACT};

pub(crate) const CONTRACT_NAME: &str = "crates.io:dao-voting-cw4";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const INSTANTIATE_GROUP_REPLY_ID: u64 = 0;

// #[cw_serde]
// pub struct Cw4GroupInstantiateMsg {
//     /// The admin is the only account that can update the group state.
//     /// Omit it to make the group immutable.
//     pub admin: Option<String>,
//     pub members: Vec<Member>,
// }

#[cw_serde]
struct Cw4GroupInstantiateMsg(cw4_group::msg::InstantiateMsg);

impl InitCallback for Cw4GroupInstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    DAO.save(
        deps.storage,
        &AnyContractInfo {
            code_hash: msg.dao_code_hash,
            addr: info.sender.clone(),
        },
    )?;

    match msg.group_contract {
        GroupContract::New {
            cw4_group_code_id,
            cw4_group_code_hash,
            initial_members,
        } => {
            if initial_members.is_empty() {
                return Err(ContractError::NoMembers {});
            }
            let original_len = initial_members.len();
            let mut initial_members = initial_members;
            initial_members.sort_by(|a, b| a.addr.cmp(&b.addr));
            initial_members.dedup();
            let new_len = initial_members.len();

            if original_len != new_len {
                return Err(ContractError::DuplicateMembers {});
            }

            let mut total_weight = Uint128::zero();
            for member in initial_members.iter() {
                deps.api.addr_validate(&member.addr)?;
                if member.weight > 0 {
                    // This works because query_voting_power_at_height will return 0 on address missing
                    // from storage, so no need to store anything.
                    let weight = Uint128::from(member.weight);
                    total_weight += weight;
                }
            }

            if total_weight.is_zero() {
                return Err(ContractError::ZeroTotalWeight {});
            }

            // Instantiate group contract, set DAO as admin.
            // Voting module contracts are instantiated by the main dao-dao-core
            // contract, so the Admin is set to info.sender.
            let msg = Cw4GroupInstantiateMsg(cw4_group::msg::InstantiateMsg {
                admin: Some(info.sender.to_string()),
                members: initial_members,
            });
            let sub_msg = SubMsg::reply_always(
                msg.to_cosmos_msg(
                    Some(info.sender.to_string()),
                    env.contract.address.to_string(),
                    cw4_group_code_id,
                    cw4_group_code_hash,
                    None,
                )?,
                INSTANTIATE_GROUP_REPLY_ID,
            );

            Ok(Response::new()
                .add_attribute("action", "instantiate")
                .add_submessage(sub_msg)
                .set_data(to_binary(&InstantiateMsgResponse {
                    code_hash: env.contract.code_hash,
                    address: env.contract.address.to_string(),
                })?))
        }
        GroupContract::Existing { address, code_hash } => {
            let group_contract = deps.api.addr_validate(&address.clone())?;

            // Validate valid group contract that has at least one member.
            let res: MemberListResponse = deps.querier.query_wasm_smart(
                code_hash.clone(),
                group_contract.clone(),
                &cw4_group::msg::QueryMsg::ListMembers {
                    start_after: None,
                    limit: Some(1),
                },
            )?;

            if res.members.is_empty() {
                return Err(ContractError::NoMembers {});
            }

            let data = AnyContractInfo {
                addr: deps.api.addr_validate(&address)?,
                code_hash: code_hash.clone(),
            };

            GROUP_CONTRACT.save(deps.storage, &data)?;

            Ok(Response::new()
                .add_attribute("action", "instantiate")
                .add_attribute("group_contract", group_contract.to_string())
                .set_data(to_binary(&InstantiateMsgResponse {
                    code_hash: env.contract.code_hash,
                    address: env.contract.address.to_string(),
                })?))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    Err(ContractError::NoExecute {})
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::VotingPowerAtHeight { address, height } => {
            query_voting_power_at_height(deps, env, address, height)
        }
        QueryMsg::TotalPowerAtHeight { height } => query_total_power_at_height(deps, env, height),
        QueryMsg::Info {} => query_info(deps),
        QueryMsg::GroupContract {} => to_binary(&GROUP_CONTRACT.load(deps.storage)?),
        QueryMsg::Dao {} => to_binary(&DAO.load(deps.storage)?),
    }
}

pub fn query_voting_power_at_height(
    deps: Deps,
    env: Env,
    address: String,
    height: Option<u64>,
) -> StdResult<Binary> {
    let addr = deps.api.addr_validate(&address)?.to_string();
    let group_contract = GROUP_CONTRACT.load(deps.storage)?;
    let res: MemberResponse = deps.querier.query_wasm_smart(
        group_contract.code_hash,
        group_contract.addr,
        &cw4_group::msg::QueryMsg::Member {
            addr,
            at_height: height,
        },
    )?;

    to_binary(&dao_interface::voting::VotingPowerAtHeightResponse {
        power: res.weight.unwrap_or(0).into(),
        height: height.unwrap_or(env.block.height),
    })
}

pub fn query_total_power_at_height(deps: Deps, env: Env, height: Option<u64>) -> StdResult<Binary> {
    let group_contract = GROUP_CONTRACT.load(deps.storage)?;
    let res: TotalWeightResponse = deps.querier.query_wasm_smart(
        group_contract.code_hash,
        group_contract.addr,
        &cw4_group::msg::QueryMsg::TotalWeight { at_height: height },
    )?;
    to_binary(&dao_interface::voting::TotalPowerAtHeightResponse {
        power: res.weight.into(),
        height: height.unwrap_or(env.block.height),
    })
}

pub fn query_info(deps: Deps) -> StdResult<Binary> {
    let info = secret_cw2::get_contract_version(deps.storage)?;
    to_binary(&dao_interface::voting::InfoResponse { info })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let storage_version: ContractVersion = get_contract_version(deps.storage)?;

    // Only migrate if newer
    if storage_version.version.as_str() < CONTRACT_VERSION {
        // Set contract to version to latest
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    }

    Ok(Response::new().add_attribute("action", "migrate"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_GROUP_REPLY_ID => match msg.result {
            SubMsgResult::Ok(res) => {
                let group_contract = GROUP_CONTRACT.may_load(deps.storage)?;
                if group_contract.is_some() {
                    return Err(ContractError::DuplicateGroupContract {});
                }
                let data: cw4_group::msg::InstantiateMsgResponse = from_binary(&res.data.unwrap())?;

                GROUP_CONTRACT.save(
                    deps.storage,
                    &AnyContractInfo {
                        addr: deps.api.addr_validate(&data.address)?,
                        code_hash: data.code_hash,
                    },
                )?;
                Ok(Response::default().add_attribute("group_contract", data.address.clone()))
            }
            SubMsgResult::Err(_) => Err(ContractError::GroupContractInstantiateError {}),
        },
        _ => Err(ContractError::UnknownReplyId { id: msg.id }),
    }
}
