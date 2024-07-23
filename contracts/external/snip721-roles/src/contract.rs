#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Response,
    StdError, StdResult, SubMsg, Uint64,
};
use cw4::{
    Member, MemberChangedHookMsg, MemberDiff, MemberListResponse, MemberResponse,
    TotalWeightResponse,
};
use dao_snip721_extensions::roles::{ExecuteExt, MetadataExt, QueryExt};
use secret_cw_controllers::HookItem;
use shade_protocol::basic_staking::{Auth, AuthPermit};
use shade_protocol::query_auth::helpers::{
    authenticate_permit, authenticate_vk, PermitAuthentication,
};
use shade_protocol::Contract;
use snip721_roles_impl::msg::NftInfo;
use snip721_roles_impl::{
    msg::InstantiateMsg as Snip721BaseInstantiateMsg, state::Snip721Contract,
};
use std::cmp::Ordering;

use crate::msg::{ExecuteMsg, QueryMsg};
use crate::state::{MembersStore, TotalStore, MEMBERS_PRIMARY};
use crate::{error::RolesContractError as ContractError, state::HOOKS};

// Version info for migration
const CONTRACT_NAME: &str = "crates.io:cw721-roles";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Settings for query pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

pub type Snip721roles = Snip721Contract<Empty, ExecuteExt, QueryExt, MetadataExt>;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: Snip721BaseInstantiateMsg,
) -> Result<Response, ContractError> {
    Snip721roles::default().instantiate(deps.branch(), env.clone(), info.clone(), msg)?;

    // Initialize total weight to zero
    TotalStore::save(deps.storage, env.block.height, 0)?;

    cw_ownable::initialize_owner(deps.storage, deps.api, Some(info.sender.as_ref()))?;

    secret_cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default()
        .add_attribute("contract_name", CONTRACT_NAME)
        .add_attribute("contract_version", CONTRACT_VERSION))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    // Only owner / minter can execute
    cw_ownable::assert_owner(deps.storage, &info.sender)?;

    match msg {
        ExecuteMsg::MintNft {
            token_id,
            owner,
            public_metadata,
            private_metadata,
            serial_number,
            royalty_info,
            transferable,
            memo,
            padding,
            extension,
        } => execute_mint(
            deps,
            env,
            info,
            token_id,
            owner,
            public_metadata,
            private_metadata,
            serial_number,
            royalty_info,
            transferable,
            memo,
            padding,
            extension,
        ),
        ExecuteMsg::BurnNft {
            token_id,
            memo,
            padding,
        } => execute_burn(deps, env, info, token_id, memo, padding),
        ExecuteMsg::Extension { msg } => match msg {
            ExecuteExt::AddHook { addr, code_hash } => {
                execute_add_hook(deps, info, addr, code_hash)
            }
            ExecuteExt::RemoveHook { addr, code_hash } => {
                execute_remove_hook(deps, info, addr, code_hash)
            }
            ExecuteExt::UpdateTokenRole { token_id, role } => {
                execute_update_token_role(deps, env, info, token_id, role)
            }
            ExecuteExt::UpdateTokenUri {
                token_id,
                token_uri,
            } => execute_update_token_uri(deps, env, info, token_id, token_uri),
            ExecuteExt::UpdateTokenWeight { token_id, weight } => {
                execute_update_token_weight(deps, env, info, token_id, weight)
            }
        },
        ExecuteMsg::TransferNft {
            recipient,
            token_id,
            memo,
            padding,
        } => execute_transfer(deps, env, info, recipient, token_id, memo, padding),
        ExecuteMsg::SendNft {
            contract,
            receiver_info,
            token_id,
            msg,
            memo,
            padding,
        } => execute_send(
            deps,
            env,
            info,
            contract,
            receiver_info,
            token_id,
            msg,
            padding,
            memo,
        ),
        _ => Snip721roles::default()
            .execute(deps, env, info, msg)
            .map_err(Into::into),
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(unused_assignments)]
pub fn execute_mint(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token_id: Option<String>,
    owner: Option<String>,
    public_metadata: Option<snip721_roles_impl::token::Metadata>,
    private_metadata: Option<snip721_roles_impl::token::Metadata>,
    serial_number: Option<snip721_roles_impl::mint_run::SerialNumber>,
    royalty_info: Option<snip721_roles_impl::royalties::RoyaltyInfo>,
    transferable: Option<bool>,
    memo: Option<String>,
    padding: Option<String>,
    extension: MetadataExt,
) -> Result<Response, ContractError> {
    let mut total = Uint64::from(TotalStore::load(deps.storage));
    let mut diff = MemberDiff::new(
        owner.clone().unwrap_or(info.sender.clone().to_string()),
        None,
        None,
    );
    let old = MembersStore::load(
        deps.storage,
        deps.api
            .addr_validate(&owner.clone().unwrap_or(info.sender.clone().to_string()))?,
    );
    // // Increment the total weight by the weight of the new token
    total = total.checked_add(Uint64::from(extension.weight))?;
    // // Add the new NFT weight to the old weight for the owner
    let new_weight = old + extension.weight;
    // // Set the diff for use in hooks
    diff = MemberDiff::new(
        owner.clone().unwrap_or(info.sender.clone().to_string()),
        Some(old),
        Some(new_weight),
    );

    MembersStore::save(
        deps.storage,
        env.block.height,
        deps.api
            .addr_validate(&owner.clone().unwrap_or(info.sender.clone().to_string()))?,
        new_weight,
    )?;
    TotalStore::save(deps.storage, env.block.height, total.u64())?;

    let diffs = MemberChangedHookMsg { diffs: vec![diff] };

    // Prepare hook messages
    let msgs = HOOKS.prepare_hooks(deps.storage, |h| {
        diffs
            .clone()
            .into_cosmos_msg(h.addr, h.code_hash)
            .map(SubMsg::new)
    })?;
    // Call base mint
    let res = Snip721roles::default().execute(
        deps,
        env,
        info,
        ExecuteMsg::MintNft {
            token_id,
            owner,
            public_metadata,
            private_metadata,
            serial_number,
            royalty_info,
            transferable,
            memo,
            padding,
            extension,
        },
    )?;

    Ok(res.add_submessages(msgs))
}

pub fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token_id: String,
    memo: Option<String>,
    padding: Option<String>,
) -> Result<Response, ContractError> {
    // // Lookup the owner of the NFT
    // let owner: OwnerOf = from_binary(&Snip721roles::default().query(
    //     deps.as_ref(),
    //     env.clone(),
    //     QueryMsg::OwnerOf {
    //         token_id: token_id.clone(),
    //         include_expired: None,
    //         viewer: None,
    //     },
    // )?)?;

    // Get the weight of the token
    let nft_info: NftInfo<MetadataExt> = from_binary(&Snip721roles::default().query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::NftInfo {
            token_id: token_id.clone(),
        },
    )?)?;

    let mut total = Uint64::from(TotalStore::load(deps.storage));
    let mut diff = MemberDiff::new(info.sender.clone(), None, None);
    let _ = diff; // reading the value in diff so we don't get warning

    // Update member weights and total
    let old_weight = MembersStore::load(deps.storage, info.sender.clone());

    // Subtract the nft weight from the member's old weight
    let new_weight = old_weight
        .checked_sub(nft_info.metadata_extension.weight)
        .ok_or(ContractError::CannotBurn {})?;

    // Subtract nft weight from the total
    total = total.checked_sub(Uint64::from(nft_info.metadata_extension.weight))?;

    // Check if the new weight is now zero
    if new_weight == 0 {
        // New weight is now None
        diff = MemberDiff::new(info.sender.clone(), Some(old_weight), None);
        // Remove owner from list of members
        MembersStore::remove(deps.storage, info.sender.clone())?;
    } else {
        let old = MembersStore::load(deps.storage, info.sender.clone());
        diff = MemberDiff::new(info.sender.clone(), Some(old), Some(new_weight));
        MembersStore::save(
            deps.storage,
            env.block.height,
            info.sender.clone(),
            new_weight,
        )?;
    }

    TotalStore::save(deps.storage, env.block.height, total.u64())?;

    let diffs = MemberChangedHookMsg { diffs: vec![diff] };

    // Prepare hook messages
    let msgs = HOOKS.prepare_hooks(deps.storage, |h| {
        diffs
            .clone()
            .into_cosmos_msg(h.addr, h.code_hash)
            .map(SubMsg::new)
    })?;

    // Burn the token
    Snip721roles::default().execute(
        deps,
        env,
        info.clone(),
        ExecuteMsg::BurnNft {
            token_id: token_id.clone(),
            memo,
            padding,
        },
    )?;

    Ok(Response::new()
        .add_attribute("action", "burn")
        .add_attribute("sender", info.sender)
        .add_attribute("token_id", token_id)
        .add_submessages(msgs))
}

pub fn execute_transfer(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    token_id: String,
    memo: Option<String>,
    padding: Option<String>,
) -> Result<Response, ContractError> {
    let contract = Snip721roles::default();

    contract.execute(
        deps,
        env,
        info.clone(),
        ExecuteMsg::TransferNft {
            recipient: recipient.clone(),
            token_id: token_id.clone(),
            memo,
            padding,
        },
    )?;

    Ok(Response::new()
        .add_attribute("action", "transfer_nft")
        .add_attribute("sender", info.sender)
        .add_attribute("recipient", recipient)
        .add_attribute("token_id", token_id))
}

#[allow(clippy::too_many_arguments)]
pub fn execute_send(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient_contract: String,
    receiver_info: Option<snip721_roles_impl::msg::ReceiverInfo>,
    token_id: String,
    msg: Option<Binary>,
    memo: Option<String>,
    padding: Option<String>,
) -> Result<Response, ContractError> {
    let contract = Snip721roles::default();

    contract.execute(
        deps,
        env,
        info.clone(),
        ExecuteMsg::SendNft {
            contract: recipient_contract.clone(),
            receiver_info,
            token_id: token_id.clone(),
            msg,
            memo,
            padding,
        },
    )?;

    Ok(Response::new()
        .add_attribute("action", "send_nft")
        .add_attribute("sender", info.sender)
        .add_attribute("recipient", recipient_contract)
        .add_attribute("token_id", token_id))
}

pub fn execute_add_hook(
    deps: DepsMut,
    _info: MessageInfo,
    addr: String,
    code_hash: String,
) -> Result<Response, ContractError> {
    let address = deps.api.addr_validate(&addr)?;
    HOOKS.add_hook(
        deps.storage,
        HookItem {
            addr: address,
            code_hash,
        },
    )?;

    Ok(Response::default()
        .add_attribute("action", "add_hook")
        .add_attribute("hook", addr))
}

pub fn execute_remove_hook(
    deps: DepsMut,
    _info: MessageInfo,
    addr: String,
    code_hash: String,
) -> Result<Response, ContractError> {
    let address = deps.api.addr_validate(&addr)?;
    HOOKS.remove_hook(
        deps.storage,
        HookItem {
            addr: address,
            code_hash,
        },
    )?;

    Ok(Response::default()
        .add_attribute("action", "remove_hook")
        .add_attribute("hook", addr))
}

pub fn execute_update_token_role(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    token_id: String,
    role: Option<String>,
) -> Result<Response, ContractError> {
    let contract = Snip721roles::default();

    // Make sure NFT exists
    let token = contract.token_extension_info.get(deps.storage, &token_id);
    if token.is_none() {
        return Err(ContractError::NftDoesNotExist {});
    }

    // Update role with new value
    token.clone().unwrap().role = role.clone();
    contract
        .token_extension_info
        .insert(deps.storage, &token_id, &token.unwrap())?;

    Ok(Response::default()
        .add_attribute("action", "update_token_role")
        .add_attribute("sender", info.sender)
        .add_attribute("token_id", token_id)
        .add_attribute("role", role.unwrap_or_default()))
}

pub fn execute_update_token_uri(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    token_id: String,
    token_uri: Option<String>,
) -> Result<Response, ContractError> {
    let contract = Snip721roles::default();

    let pub_metdata = contract.pub_metadata.get(deps.storage, &token_id);
    if pub_metdata.is_none() {
        return Err(ContractError::NftDoesNotExist {});
    }
    let priv_metdata = contract.priv_metadata.get(deps.storage, &token_id);
    if priv_metdata.is_none() {
        return Err(ContractError::NftDoesNotExist {});
    }

    // Set new token URI
    pub_metdata.clone().unwrap().token_uri = token_uri.clone();
    priv_metdata.clone().unwrap().token_uri = token_uri.clone();
    contract
        .pub_metadata
        .insert(deps.storage, &token_id, &pub_metdata.unwrap())?;
    contract
        .priv_metadata
        .insert(deps.storage, &token_id, &priv_metdata.unwrap())?;

    Ok(Response::new()
        .add_attribute("action", "update_token_uri")
        .add_attribute("sender", info.sender)
        .add_attribute("token_id", token_id)
        .add_attribute("token_uri", token_uri.unwrap_or_default()))
}

pub fn execute_update_token_weight(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token_id: String,
    weight: u64,
) -> Result<Response, ContractError> {
    let contract = Snip721roles::default();

    // Make sure NFT exists
    let token = contract.token_extension_info.get(deps.storage, &token_id);
    if token.is_none() {
        return Err(ContractError::NftDoesNotExist {});
    }

    // // Lookup the owner of the NFT
    // let owner: OwnerOf = from_binary(&contract.query(
    //     deps.as_ref(),
    //     env.clone(),
    //     snip721_roles_impl::msg::QueryMsg::OwnerOf {
    //         token_id: token_id.clone(),
    //         viewer: None,
    //         include_expired: None,
    //     },
    // )?)?;

    let mut total = Uint64::from(TotalStore::load(deps.storage));
    let mut diff = MemberDiff::new(info.sender.clone(), None, None);

    // Update member weights and total
    let old = MembersStore::load(deps.storage, info.sender.clone());
    let new_total_weight;
    let old_total_weight = old;

    match weight.cmp(&token.clone().unwrap().weight) {
        Ordering::Greater => {
            // Subtract the old token weight from the new token weight
            let weight_difference = weight
                .checked_sub(token.clone().unwrap().weight)
                .ok_or(ContractError::NegativeValue {})?;

            // Increment the total weight by the weight difference of the new token
            total = total.checked_add(Uint64::from(weight_difference))?;
            // Add the new NFT weight to the old weight for the owner
            new_total_weight = old_total_weight + weight_difference;
            // Set the diff for use in hooks
            diff = MemberDiff::new(info.sender.clone(), Some(old), Some(new_total_weight));
        }
        Ordering::Less => {
            // Subtract the new token weight from the old token weight
            let weight_difference = token
                .clone()
                .unwrap()
                .weight
                .checked_sub(weight)
                .ok_or(ContractError::NegativeValue {})?;

            // Subtract the weight difference from the old total weight
            new_total_weight = old_total_weight
                .checked_sub(weight_difference)
                .ok_or(ContractError::NegativeValue {})?;

            // Subtract difference from the total
            total = total.checked_sub(Uint64::from(weight_difference))?;
        }
        Ordering::Equal => return Err(ContractError::NoWeightChange {}),
    }
    MembersStore::save(
        deps.storage,
        env.block.height,
        info.sender.clone(),
        new_total_weight,
    )?;

    TotalStore::save(deps.storage, env.block.height, total.u64())?;

    let diffs = MemberChangedHookMsg { diffs: vec![diff] };

    // Prepare hook messages
    let msgs = HOOKS.prepare_hooks(deps.storage, |h| {
        diffs
            .clone()
            .into_cosmos_msg(h.addr, h.code_hash)
            .map(SubMsg::new)
    })?;

    // Save token weight
    token.clone().unwrap().weight = weight;
    contract
        .token_extension_info
        .insert(deps.storage, &token_id, &token.unwrap())?;

    Ok(Response::default()
        .add_submessages(msgs)
        .add_attribute("action", "update_token_weight")
        .add_attribute("sender", info.sender)
        .add_attribute("token_id", token_id)
        .add_attribute("weight", weight.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::QueryExtension { msg } => match msg {
            QueryExt::Hooks {} => to_binary(&HOOKS.query_hooks(deps)?),
            QueryExt::ListMembers { start_after, limit } => {
                to_binary(&query_list_members(deps, start_after, limit)?)
            }
            QueryExt::Member { at_height, auth } => {
                let query_auth = Snip721roles::default().query_auth.load(deps.storage)?;
                let user = authenticate(deps, auth, query_auth)?;
                to_binary(&query_member(deps, user, at_height)?)
            }
            QueryExt::TotalWeight { at_height } => to_binary(&query_total_weight(deps, at_height)?),
        },
        _ => Snip721roles::default().query(deps, env, msg),
    }
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

pub fn query_member(deps: Deps, addr: Addr, height: Option<u64>) -> StdResult<MemberResponse> {
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

pub fn query_list_members(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<MemberListResponse> {
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

pub fn authenticate(deps: Deps, auth: Auth, query_auth: Contract) -> StdResult<Addr> {
    match auth {
        Auth::ViewingKey { key, address } => {
            let address = deps.api.addr_validate(&address)?;
            if !authenticate_vk(address.clone(), key, &deps.querier, &query_auth)? {
                return Err(StdError::generic_err("Invalid Viewing Key"));
            }
            Ok(address)
        }
        Auth::Permit(permit) => {
            let res: PermitAuthentication<AuthPermit> =
                authenticate_permit(permit, &deps.querier, query_auth)?;
            if res.revoked {
                return Err(StdError::generic_err("Permit Revoked"));
            }
            Ok(res.sender)
        }
    }
}
