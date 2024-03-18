#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
    SubMsg, Uint64,
};
use cw4::{
    Member, MemberChangedHookMsg, MemberDiff, MemberListResponse, MemberResponse,
    TotalWeightResponse,
};
use secret_cw2::set_contract_version;
use secret_toolkit::permit::{Permit, RevokedPermits, TokenPermissions};
use secret_toolkit::viewing_key::{ViewingKey, ViewingKeyStore};
// use secret_storage_plus::Bound;
// use secret_utils::maybe_addr;

use crate::error::ContractError;
use crate::helpers::validate_unique_members;
use crate::msg::{
    CreateViewingKeyResponse, ExecuteMsg, InstantiateMsg, InstantiateMsgResponse, QueryMsg,
    QueryWithPermit, ViewingKeyError,
};
use crate::state::{MembersStore, TotalStore, ADMIN, HOOKS, MEMBERS_PRIMARY};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw4-group";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const PREFIX_REVOKED_PERMITS: &str = "revoked_permits";

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    create(deps, msg.admin, msg.members, env.block.height)?;
    Ok(
        Response::default().set_data(to_binary(&InstantiateMsgResponse {
            address: env.contract.address.to_string(),
            code_hash: env.contract.code_hash,
        })?),
    )
}

// create is the instantiation logic with set_contract_version removed so it can more
// easily be imported in other contracts
pub fn create(
    mut deps: DepsMut,
    admin: Option<String>,
    mut members: Vec<Member>,
    height: u64,
) -> Result<(), ContractError> {
    validate_unique_members(&mut members)?;
    let members = members; // let go of mutability

    let admin_addr = admin
        .map(|admin| deps.api.addr_validate(&admin))
        .transpose()?;
    ADMIN.set(deps.branch(), admin_addr)?;

    let mut total = Uint64::zero();
    for member in members.into_iter() {
        let member_weight = Uint64::from(member.weight);
        total = total.checked_add(member_weight)?;
        let member_addr = deps.api.addr_validate(&member.addr)?;
        MembersStore::save(deps.storage, height, member_addr, member_weight.u64())?;
    }
    TotalStore::save(deps.storage, height, total.u64())?;

    Ok(())
}

// And declare a custom Error variant for the ones where you will want to make use of it
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let api = deps.api;
    match msg {
        ExecuteMsg::UpdateAdmin { admin } => Ok(ADMIN.execute_update_admin(
            deps,
            info,
            admin.map(|admin| api.addr_validate(&admin)).transpose()?,
        )?),
        ExecuteMsg::UpdateMembers { add, remove } => {
            execute_update_members(deps, env, info, add, remove)
        }
        ExecuteMsg::AddHook { hook } => Ok(HOOKS.execute_add_hook(
            &ADMIN,
            deps,
            info,
            api.addr_validate(hook.addr.as_str())?,
            hook.code_hash,
        )?),
        ExecuteMsg::RemoveHook { hook } => Ok(HOOKS.execute_remove_hook(
            &ADMIN,
            deps,
            info,
            api.addr_validate(hook.addr.as_str())?,
            hook.code_hash,
        )?),
        ExecuteMsg::CreateViewingKey { entropy, .. } => try_create_key(deps, env, info, entropy),
        ExecuteMsg::SetViewingKey { key, .. } => try_set_key(deps, info, key),
        ExecuteMsg::RevokePermit { permit_name, .. } => revoke_permit(deps, info, permit_name),
    }
}

pub fn execute_update_members(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    add: Vec<Member>,
    remove: Vec<String>,
) -> Result<Response, ContractError> {
    let attributes = vec![
        attr("action", "update_members"),
        attr("added", add.len().to_string()),
        attr("removed", remove.len().to_string()),
        attr("sender", &info.sender),
    ];

    // make the local update
    let diff = update_members(deps.branch(), env.block.height, info.sender, add, remove)?;
    // call all registered hooks
    let messages = HOOKS.prepare_hooks(deps.storage, |hook| {
        diff.clone()
            .into_cosmos_msg(hook.addr, hook.code_hash)
            .map(SubMsg::new)
    })?;
    Ok(Response::new()
        .add_submessages(messages)
        .add_attributes(attributes))
}

// the logic from execute_update_members extracted for easier import
pub fn update_members(
    deps: DepsMut,
    height: u64,
    sender: Addr,
    mut to_add: Vec<Member>,
    to_remove: Vec<String>,
) -> Result<MemberChangedHookMsg, ContractError> {
    validate_unique_members(&mut to_add)?;
    let to_add = to_add; // let go of mutability

    ADMIN.assert_admin(deps.as_ref(), &sender)?;

    let mut total = Uint64::from(TotalStore::load(deps.storage));
    let mut diffs: Vec<MemberDiff> = vec![];

    // add all new members and update total
    for add in to_add.into_iter() {
        let add_addr = deps.api.addr_validate(&add.addr)?;
        // MEMBERS.update(deps.storage, &add_addr, height, |old| -> StdResult<_> {
        //     total = total.checked_sub(Uint64::from(old.unwrap_or_default()))?;
        //     total = total.checked_add(Uint64::from(add.weight))?;
        //     diffs.push(MemberDiff::new(add.addr, old, Some(add.weight)));
        //     Ok(add.weight)
        // })?;
        let old = MembersStore::load(deps.storage, add_addr.clone());
        total = total.checked_sub(Uint64::from(old))?;
        total = total.checked_add(Uint64::from(add.weight))?;
        diffs.push(MemberDiff::new(add.addr, Some(old), Some(add.weight)));
        MembersStore::save(deps.storage, height, add_addr.clone(), add.weight)?
    }

    for remove in to_remove.into_iter() {
        let remove_addr = deps.api.addr_validate(&remove)?;
        let old = MembersStore::load(deps.storage, remove_addr.clone());
        // Only process this if they were actually in the list before
        let weight = old;
        diffs.push(MemberDiff::new(remove, Some(weight), None));
        total = total.checked_sub(Uint64::from(weight))?;
        MembersStore::remove(deps.storage, remove_addr.clone())?;
    }

    TotalStore::save(deps.storage, height, total.u64())?;
    Ok(MemberChangedHookMsg { diffs })
}

pub fn try_create_key(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    entropy: String,
) -> Result<Response, ContractError> {
    let key = ViewingKey::create(
        deps.storage,
        &info,
        &env,
        info.sender.as_str(),
        entropy.as_ref(),
    );

    Ok(Response::new().set_data(to_binary(&CreateViewingKeyResponse { key })?))
}

pub fn try_set_key(
    deps: DepsMut,
    info: MessageInfo,
    key: String,
) -> Result<Response, ContractError> {
    ViewingKey::set(deps.storage, info.sender.as_str(), key.as_str());
    Ok(Response::default())
}

fn revoke_permit(
    deps: DepsMut,
    info: MessageInfo,
    permit_name: String,
) -> Result<Response, ContractError> {
    RevokedPermits::revoke_permit(
        deps.storage,
        PREFIX_REVOKED_PERMITS,
        info.sender.as_str(),
        &permit_name,
    );

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::ListMembers { start_after, limit } => {
            to_binary(&query_list_members(deps, start_after, limit)?)
        }
        QueryMsg::TotalWeight { at_height: height } => {
            to_binary(&query_total_weight(deps, height)?)
        }
        QueryMsg::Admin {} => to_binary(&ADMIN.query_admin(deps)?),
        QueryMsg::Hooks {} => to_binary(&HOOKS.query_hooks(deps)?),
        QueryMsg::WithPermit { permit, query } => permit_queries(deps, env, permit, query),
        _ => viewing_keys_queries(deps, env, msg),
    }
}

fn permit_queries(
    deps: Deps,
    env: Env,
    permit: Permit,
    query: QueryWithPermit,
) -> Result<Binary, StdError> {
    // Validate permit content
    let _account = secret_toolkit::permit::validate(
        deps,
        PREFIX_REVOKED_PERMITS,
        &permit,
        env.contract.address.clone().into_string(),
        None,
    )?;

    // Permit validated! We can now execute the query.
    match query {
        QueryWithPermit::Member { address, at_height } => {
            if !permit.check_permission(&TokenPermissions::Balance) {
                return Err(StdError::generic_err(format!(
                    "No permission to query balance, got permissions {:?}",
                    permit.params.permissions
                )));
            }

            to_binary(&query_member(deps, address, at_height)?)
        }
    }
}

pub fn viewing_keys_queries(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let (addresses, key) = msg.get_validation_params(deps.api)?;

    for address in addresses {
        let result = ViewingKey::check(deps.storage, address.as_str(), key.as_str());
        if result.is_ok() {
            return match msg {
                // Base
                QueryMsg::Member {
                    addr, at_height, ..
                } => to_binary(&query_member(deps, addr, at_height)?),
                _ => panic!("This query type does not require authentication"),
            };
        }
    }

    to_binary(&ViewingKeyError {
        msg: "Wrong viewing key for this address or viewing key not set".to_string(),
    })
}

pub fn query_total_weight(deps: Deps, height: Option<u64>) -> StdResult<TotalWeightResponse> {
    if height.is_some() {
        let weight = TotalStore::may_load_at_height(deps.storage, height.unwrap())?;
        Ok(TotalWeightResponse {
            weight: weight.unwrap(),
        })
    } else {
        let weight = TotalStore::load(deps.storage);
        Ok(TotalWeightResponse { weight })
    }
}

pub fn query_member(deps: Deps, addr: String, height: Option<u64>) -> StdResult<MemberResponse> {
    let addr = deps.api.addr_validate(&addr)?;
    if height.is_some() {
        let weight = MembersStore::may_load_at_height(deps.storage, addr.clone(), height.unwrap())?;

        Ok(MemberResponse { weight })
    } else {
        let weight = MembersStore::load(deps.storage, addr.clone());

        Ok(MemberResponse {
            weight: Some(weight),
        })
    }
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

pub fn query_list_members(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<MemberListResponse> {
    // let addr = maybe_addr(deps.api, start_after)?;
    // let start = addr.as_ref().map(Bound::exclusive);

    // let members = MEMBERS
    //     .range(deps.storage, start, None, Order::Ascending)
    //     .take(limit)
    //     .map(|item| {
    //         item.map(|(addr, weight)| Member {
    //             addr: addr.into(),
    //             weight,
    //         })
    //     })
    //     .collect::<StdResult<_>>()?;
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;

    let mut res_members: Vec<Member> = Vec::new();

    let mut start = start_after.clone(); // Clone start_after to mutate it if necessary

    let binding = &MEMBERS_PRIMARY;
    let iter = binding.iter(deps.storage)?;
    for item in iter {
        let (address, weight) = item?;
        if let Some(start_after) = &start {
            if &address == start_after {
                // If we found the start point, reset it to start iterating
                start = None;
            }
        }
        if start.is_none() {
            res_members.push(Member {
                addr: address.to_string(),
                weight,
            });
            if res_members.len() >= limit {
                break; // Break out of loop if limit reached
            }
        }
    }

    let response = MemberListResponse {
        members: res_members,
    };

    Ok(response)
}
