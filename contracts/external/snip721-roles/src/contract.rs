use crate::msg::{ExecuteMsg, InstantiateMsg, InstantiateResponse, QueryMsg};
use crate::snip721::{self, Extension, Metadata, ReceiverInfo, Snip721ExecuteMsg, Snip721QueryMsg};
use cosmwasm_schema::serde;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Response,
    StdResult, SubMsg, Uint64, WasmMsg,
};
use cw4::{
    Member, MemberChangedHookMsg, MemberDiff, MemberListResponse, MemberResponse,
    TotalWeightResponse,
};
use schemars::JsonSchema;
use secret_toolkit::utils::InitCallback;
use serde::{Deserialize, Serialize};
// use cw721_base::Cw721Contract;
// use snip721_reference_impl::msg::InstantiateMsg as Cw721BaseInstantiateMsg;

use dao_snip721_extensions::roles::{ExecuteExt, MetadataExt, QueryExt};
use std::cmp::Ordering;

use crate::state::{Config, MembersStore, TotalStore, SNIP721_INFO};
use crate::{error::RolesContractError as ContractError, state::HOOKS};

// Version info for migration
const CONTRACT_NAME: &str = "crates.io:snip721-roles";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Settings for query pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Snip721ReceiveMsg {
    /// ReceiveNft may be a HandleMsg variant of any contract that wants to implement a receiver
    /// interface.  BatchReceiveNft, which is more informative and more efficient, is preferred over
    /// ReceiveNft.  Please read above regarding why ReceiveNft, which follows CW-721 standard has an
    /// inaccurately named `sender` field
    ReceiveNft {
        /// previous owner of sent token
        sender: Addr,
        /// token that was sent
        token_id: String,
        /// optional message to control receiving logic
        msg: Option<Binary>,
    },
    /// BatchReceiveNft may be a HandleMsg variant of any contract that wants to implement a receiver
    /// interface.  BatchReceiveNft, which is more informative and more efficient, is preferred over
    /// ReceiveNft.
    BatchReceiveNft {
        /// address that sent the tokens.  There is no ReceiveNft field equivalent to this
        sender: Addr,
        /// previous owner of sent tokens.  This is equivalent to the ReceiveNft `sender` field
        from: Addr,
        /// tokens that were sent
        token_ids: Vec<String>,
        /// optional message to control receiving logic
        msg: Option<Binary>,
    },
}

const SNIP721_INIT_ID: u64 = 0;

// pub type Cw721Roles<'a> = Cw721Contract<'a, MetadataExt, Empty, ExecuteExt, QueryExt>;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
     deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    // Cw721Roles::default().instantiate(deps.branch(), env.clone(), info, msg)?;

    // init snip721
    let submsg = SubMsg::reply_always(
        msg.snip721_init_msg.to_cosmos_msg(
            Some(info.sender.clone().to_string()),
            msg.label,
            msg.snip721_code_id,
            msg.snip721_code_hash.clone(),
            None,
        )?,
        SNIP721_INIT_ID,
    );

    // Initialize total weight to zero
    TotalStore::save(deps.storage, env.block.height, 0)?;
    SNIP721_INFO.save(
        deps.storage,
        &Config {
            code_hash: msg.snip721_code_hash.clone(),
            contract_address: Addr::unchecked(""),
        },
    )?;

    secret_cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default()
        .add_attribute("contract_name", CONTRACT_NAME)
        .add_attribute("contract_version", CONTRACT_VERSION)
        .add_submessage(submsg))
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
        ExecuteMsg::Snip721Execute(ref snip721_exec_msg) => match snip721_exec_msg {
            snip721_reference_impl::msg::ExecuteMsg::MintNft {
                token_id,
                owner,
                public_metadata,
                private_metadata,
                serial_number,
                royalty_info,
                transferable,
                memo,
                padding,
            } => execute_mint(
                deps,
                &env,
                &info.sender,
                token_id,
                owner,
                public_metadata,
                private_metadata,
                serial_number,
                royalty_info,
                transferable,
                memo,
                padding
            ),
            snip721_reference_impl::msg::ExecuteMsg::BurnNft {
                token_id,
                memo,
                padding,
            } => execute_burn(deps, env, info, token_id, memo, padding),
            snip721_reference_impl::msg::ExecuteMsg::TransferNft {
                recipient,
                token_id,
                memo,
                padding,
            } => execute_transfer(deps, env, info, recipient, token_id, memo, padding),
            snip721_reference_impl::msg::ExecuteMsg::SendNft {
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
                msg.unwrap(),
                memo,
                padding,
            ),
            _ => {
                let snip721_info = SNIP721_INFO.load(deps.storage)?;
                let exec_msg = WasmMsg::Execute {
                    contract_addr: snip721_info.contract_address.to_string(),
                    code_hash: snip721_info.code_hash,
                    msg: to_binary(&msg)?,
                    funds: vec![],
                };
                Ok(Response::default().add_message(exec_msg))
            }
        },
        ExecuteMsg::ExtensionExecute(extension_msg) => match extension_msg {
            ExecuteExt::AddHook { addr } => execute_add_hook(deps, info, addr),
            ExecuteExt::RemoveHook { addr } => execute_remove_hook(deps, info, addr),
            ExecuteExt::UpdateTokenUri {
                token_id,
                token_uri,
            } => execute_update_token_uri(deps, env, info, token_id, token_uri),
            ExecuteExt::UpdateTokenWeight { token_id, weight } => {
                execute_update_token_weight(deps, env, info, token_id, weight)
            }
            ExecuteExt::UpdateTokenRole { token_id, role } => {
                execute_update_token_role(deps, env, info, token_id, role)
            }
        },
    }
}

pub fn execute_mint(
    deps: DepsMut,
    env: &Env,
    sender: &Addr,
    token_id: Option<String>,
    owner: Option<String>,
    public_metadata: Option<snip721_reference_impl::token::Metadata>,
    private_metadata: Option<snip721_reference_impl::token::Metadata>,
    serial_number: Option<snip721_reference_impl::mint_run::SerialNumber>,
    royalty_info: Option<snip721_reference_impl::royalties::RoyaltyInfo>,
    transferable: Option<bool>,
    memo: Option<String>,
    padding: Option<String>,
) -> Result<Response, ContractError> {
    let snip721_info = SNIP721_INFO.load(deps.storage)?;
    let mut total = Uint64::from(TotalStore::load(deps.storage));
    let mut diff = MemberDiff::new(owner.clone().unwrap(), None, None);
    let old = MembersStore::load(deps.storage, deps.api.addr_validate(&owner.clone().unwrap())?);
    // Increment the total weight by the weight of the new token
    total = total.checked_add(Uint64::from(
        public_metadata.unwrap().extension.unwrap().weight,
    ))?;
    // Add the new NFT weight to the old weight for the owner
    let new_weight = old+ public_metadata.unwrap().extension.unwrap().weight;
    // Set the diff for use in hooks
    diff = MemberDiff::new(owner.clone().unwrap(), Some(old), Some(new_weight));
    // Update member weights and total

    // MEMBERS.update(
    //     deps.storage,
    //     &deps.api.addr_validate(&owner)?,
    //     env.block.height,
    //     |old| -> StdResult<_> {
    //         // Increment the total weight by the weight of the new token
    //         total = total.checked_add(Uint64::from(extension.weight))?;
    //         // Add the new NFT weight to the old weight for the owner
    //         let new_weight = old.unwrap_or_default() + extension.unwrap().weight;
    //         // Set the diff for use in hooks
    //         diff = MemberDiff::new(owner.clone(), old, Some(new_weight));
    //         Ok(new_weight)
    //     },
    // )?;
    MembersStore::save(
        deps.storage,
        env.block.height,
        deps.api.addr_validate(&owner.unwrap())?.clone(),
        new_weight,
    );
    TotalStore::save(deps.storage, env.block.height, total.u64())?;

    let diffs = MemberChangedHookMsg { diffs: vec![diff] };

    // Prepare hook messages
    let msgs = HOOKS.prepare_hooks(deps.storage, |h| {
        diffs.clone().into_cosmos_msg(h,env.contract.code_hash.clone()).map(SubMsg::new)
    })?;

    // Call Snip721 mint
    let exec_msg = WasmMsg::Execute {
        contract_addr: snip721_info.contract_address.to_string(),
        code_hash: snip721_info.code_hash,
        msg: to_binary(&snip721_reference_impl::msg::ExecuteMsg::MintNft {
            token_id,
            owner,
            public_metadata,
            private_metadata,
            serial_number,
            royalty_info,
            transferable,
            memo,
            padding,
        })?,
        funds: vec![],
    };

    Ok(Response::default()
        .add_submessages(msgs)
        .add_message(exec_msg))
}

pub fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token_id: String,
    memo: Option<String>,
    padding: Option<String>,
) -> Result<Response, ContractError> {
    let snip721_info = SNIP721_INFO.load(deps.storage)?;
    // Lookup the owner of the NFT
    let owner: snip721_reference_impl::msg::OwnerOf = deps.querier.query_wasm_smart(
        snip721_info.code_hash.clone(),
        snip721_info.contract_address.to_string().clone(),
        &Snip721QueryMsg::OwnerOf {
            token_id,
            viewer: None,
            include_expired: None,
        },
    )?;

    // Get the weight of the token
    let nft_info: snip721_reference_impl::msg::NftInfo = deps.querier.query_wasm_smart(
        snip721_info.code_hash.clone(),
        snip721_info.contract_address.to_string().clone(),
        &Snip721QueryMsg::NftInfo { token_id },
    )?;

    let mut total = Uint64::from(TotalStore::load(deps.storage));
    let mut diff = MemberDiff::new(owner.owner.clone(), None, None);

    // Update member weights and total
    let owner_addr = owner.owner;
    let old_weight = MembersStore::load(deps.storage, owner_addr);

    // Subtract the nft weight from the member's old weight
    let new_weight = old_weight
        .checked_sub(nft_info.extension.unwrap().weight)
        .ok_or(ContractError::CannotBurn {})?;

    // Subtract nft weight from the total
    total = total.checked_sub(Uint64::from(nft_info.extension.unwrap().weight))?;

    // Check if the new weight is now zero
    if new_weight == 0 {
        // New weight is now None
        diff = MemberDiff::new(owner.owner, Some(old_weight), None);
        // Remove owner from list of members
        MembersStore::remove(deps.storage, owner_addr.clone());
    } else {
        // MEMBERS.update(
        //     deps.storage,
        //     &owner_addr,
        //     env.block.height,
        //     |old| -> StdResult<_> {
        //         diff = MemberDiff::new(owner.owner.clone(), old, Some(new_weight));
        //         Ok(new_weight)
        //     },
        // )?;
        let old = MembersStore::load(deps.storage, owner_addr.clone());
        diff = MemberDiff::new(owner.owner.clone(), Some(old), Some(new_weight));
        MembersStore::save(
            deps.storage,
            env.block.height,
            owner_addr.clone(),
            new_weight,
        )?;
    }

    TotalStore::save(deps.storage, env.block.height, total.u64())?;

    let diffs = MemberChangedHookMsg { diffs: vec![diff] };

    // Prepare hook messages
    let msgs = HOOKS.prepare_hooks(deps.storage, |h| {
        diffs.clone().into_cosmos_msg(h,env.contract.code_hash).map(SubMsg::new)
    })?;

    // Burn the token
    let exec_msg = WasmMsg::Execute {
        contract_addr: snip721_info.contract_address.to_string().clone(),
        code_hash: snip721_info.code_hash.clone(),
        msg: to_binary(&snip721_reference_impl::msg::ExecuteMsg::BurnNft {
            token_id,
            memo: None,
            padding: None,
        })?,
        funds: vec![],
    };

    Ok(Response::new()
        .add_attribute("action", "burn")
        .add_attribute("sender", info.sender)
        .add_attribute("token_id", token_id)
        .add_submessages(msgs)
        .add_message(exec_msg))
}

pub fn execute_transfer(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient: String,
    token_id: String,
    memo: Option<String>,
    padding: Option<String>,
) -> Result<Response, ContractError> {
    let snip721_info = SNIP721_INFO.load(deps.storage)?;

    // let contract = Cw721Roles::default();
    // let mut token = contract.tokens.load(deps.storage, &token_id)?;
    // // set owner and remove existing approvals
    // token.owner = deps.api.addr_validate(&recipient)?;
    // token.approvals = vec![];
    // contract.tokens.save(deps.storage, &token_id, &token)?;

    let exec_msg = WasmMsg::Execute {
        contract_addr: snip721_info.contract_address.to_string().clone(),
        code_hash: snip721_info.code_hash.clone(),
        msg: to_binary(&snip721_reference_impl::msg::ExecuteMsg::TransferNft {
            recipient,
            token_id,
            memo: None,
            padding: None,
        })?,
        funds: vec![],
    };

    Ok(Response::new()
        .add_attribute("action", "transfer_nft")
        .add_attribute("sender", info.sender)
        .add_attribute("recipient", recipient)
        .add_attribute("token_id", token_id)
        .add_message(exec_msg))
}

pub fn execute_send(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient_contract: String,
    recipient_info: Option<snip721_reference_impl::msg::ReceiverInfo>,
    token_id: String,
    msg: Binary,
    memo: Option<String>,
    padding: Option<String>,
) -> Result<Response, ContractError> {
    let snip721_info = SNIP721_INFO.load(deps.storage)?;

    // let contract = Cw721Roles::default();
    // let mut token = contract.tokens.load(deps.storage, &token_id)?;
    // // set owner and remove existing approvals
    // token.owner = deps.api.addr_validate(&recipient_contract)?;
    // token.approvals = vec![];
    // contract.tokens.save(deps.storage, &token_id, &token)?;
    // let send = Snip721ReceiveMsg {
    //     sender: info.sender.to_string(),
    //     token_id: token_id.clone(),
    //     msg,
    // };

    let exec_msg = WasmMsg::Execute {
        contract_addr: snip721_info.contract_address.to_string().clone(),
        code_hash: snip721_info.code_hash.clone(),
        msg: to_binary(&snip721_reference_impl::msg::ExecuteMsg::SendNft {
            contract: recipient_contract,
            receiver_info: Some(snip721_reference_impl::msg::ReceiverInfo {
                recipient_code_hash: recipient_info.unwrap().recipient_code_hash,
                also_implements_batch_receive_nft: None,
            }),
            token_id,
            msg: Some(msg),
            memo: None,
            padding: None,
        })?,
        funds: vec![],
    };

    Ok(Response::new()
        .add_attribute("action", "send_nft")
        .add_attribute("sender", info.sender)
        .add_attribute("recipient", recipient_contract)
        .add_attribute("token_id", token_id)
        .add_message(exec_msg))
}

pub fn execute_add_hook(
    deps: DepsMut,
    _info: MessageInfo,
    addr: String,
) -> Result<Response, ContractError> {
    let hook = deps.api.addr_validate(&addr)?;
    HOOKS.add_hook(deps.storage, hook)?;

    Ok(Response::default()
        .add_attribute("action", "add_hook")
        .add_attribute("hook", addr))
}

pub fn execute_remove_hook(
    deps: DepsMut,
    _info: MessageInfo,
    addr: String,
) -> Result<Response, ContractError> {
    let hook = deps.api.addr_validate(&addr)?;
    HOOKS.remove_hook(deps.storage, hook)?;

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
    let snip721_info = SNIP721_INFO.load(deps.storage)?;
    // Make sure NFT exists
    let token: snip721_reference_impl::msg::NftInfo = deps
        .querier
        .query_wasm_smart(
            snip721_info.code_hash.clone(),
            snip721_info.contract_address.to_string().clone(),
            &snip721_reference_impl::msg::QueryMsg::NftInfo {
                token_id: token_id.clone(),
            },
        )
        .map_err(|_| ContractError::NftDoesNotExist {})?;

    // Update role with new value
    let exec_msg = WasmMsg::Execute {
        contract_addr: snip721_info.contract_address.to_string().clone(),
        code_hash: snip721_info.code_hash.clone(),
        msg: to_binary(&snip721_reference_impl::msg::ExecuteMsg::SetMetadata {
            token_id: token_id.clone(),
            public_metadata: Some(snip721_reference_impl::token::Metadata {
                token_uri: Some(token.token_uri.unwrap()),
                extension: Some(snip721_reference_impl::token::Extension {
                    image: Some(token.extension.clone().unwrap().image.unwrap()),
                    image_data: Some(token.extension.clone().unwrap().image_data.unwrap()),
                    external_url: Some(token.extension.clone().unwrap().external_url.unwrap()),
                    description: Some(token.extension.clone().unwrap().description.unwrap()),
                    name: Some(token.extension.clone().unwrap().name.unwrap()),
                    attributes: Some(token.extension.clone().unwrap().attributes.unwrap()),
                    background_color: Some(token.extension.clone().unwrap().background_color.unwrap()),
                    animation_url: Some(token.extension.clone().unwrap().animation_url.unwrap()),
                    youtube_url: Some(token.extension.clone().unwrap().youtube_url.unwrap()),
                    media: Some(token.extension.clone().unwrap().media.unwrap()),
                    protected_attributes: Some(token.extension.clone().unwrap().protected_attributes.unwrap()),
                    token_subtype: Some(token.extension.clone().unwrap().token_subtype.unwrap()),
                    role,
                    weight: token.extension.unwrap().weight,
                }),
            }),
            private_metadata: None,
            padding: None,
        })?,
        funds: vec![],
    };

    Ok(Response::default()
        .add_attribute("action", "update_token_role")
        .add_attribute("sender", info.sender)
        .add_attribute("token_id", token_id)
        .add_attribute("role", role.unwrap_or_default())
        .add_message(exec_msg))
}

pub fn execute_update_token_uri(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    token_id: String,
    token_uri: Option<String>,
) -> Result<Response, ContractError> {
    let snip721_info = SNIP721_INFO.load(deps.storage)?;
    // Make sure NFT exists
    let token: snip721_reference_impl::msg::NftInfo = deps
        .querier
        .query_wasm_smart(
            snip721_info.code_hash.clone(),
            snip721_info.contract_address.to_string().clone(),
            &Snip721QueryMsg::NftInfo {
                token_id: token_id.clone(),
            },
        )
        .map_err(|_| ContractError::NftDoesNotExist {})?;

    // Update role with new value
    let exec_msg = WasmMsg::Execute {
        contract_addr: snip721_info.contract_address.to_string().clone(),
        code_hash: snip721_info.code_hash.clone(),
        msg: to_binary(&snip721_reference_impl::msg::ExecuteMsg::SetMetadata {
            token_id: token_id.clone(),
            public_metadata: Some(snip721_reference_impl::token::Metadata {
                token_uri,
                extension: Some(snip721_reference_impl::token::Extension {
                    image: Some(token.extension.clone().unwrap().image.unwrap()),
                    image_data: Some(token.extension.clone().unwrap().image_data.unwrap()),
                    external_url: Some(token.extension.clone().unwrap().external_url.unwrap()),
                    description: Some(token.extension.clone().unwrap().description.unwrap()),
                    name: Some(token.extension.clone().unwrap().name.unwrap()),
                    attributes: Some(token.extension.clone().unwrap().attributes.unwrap()),
                    background_color: Some(token.extension.clone().unwrap().background_color.unwrap()),
                    animation_url: Some(token.extension.clone().unwrap().animation_url.unwrap()),
                    youtube_url: Some(token.extension.clone().unwrap().youtube_url.unwrap()),
                    media: Some(token.extension.clone().unwrap().media.unwrap()),
                    protected_attributes: Some(token.extension.clone().unwrap().protected_attributes.unwrap()),
                    token_subtype: Some(token.extension.clone().unwrap().token_subtype.unwrap()),
                    role: Some(token.extension.unwrap().role.unwrap()),
                    weight: token.extension.unwrap().weight,
                }),
            }),
            private_metadata: None,
            padding: None,
        })?,
        funds: vec![],
    };

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
    let snip721_info = SNIP721_INFO.load(deps.storage)?;
    // Make sure NFT exists
    let token: snip721_reference_impl::msg::NftInfo = deps
        .querier
        .query_wasm_smart(
            snip721_info.code_hash.clone(),
            snip721_info.contract_address.to_string().clone(),
            &Snip721QueryMsg::NftInfo {
                token_id: token_id.clone(),
            },
        )
        .map_err(|_| ContractError::NftDoesNotExist {})?;

    // Lookup the owner of the NFT
    let owner: snip721_reference_impl::msg::OwnerOf = deps.querier.query_wasm_smart(
        snip721_info.code_hash.clone(),
        snip721_info.contract_address.to_string().clone(),
        &Snip721QueryMsg::OwnerOf {
            token_id,
            viewer: None,
            include_expired: None,
        },
    )?;

    let mut total = Uint64::from(TotalStore::load(deps.storage));
    let mut diff = MemberDiff::new(owner.owner.clone(), None, None);

    // Update member weights and total
    let old = MembersStore::load(deps.storage, owner.owner.clone());
    let new_total_weight;
    let old_total_weight = old;

    match weight.cmp(&token.extension.unwrap().weight) {
        Ordering::Greater => {
            // Subtract the old token weight from the new token weight
            let weight_difference = weight
                .checked_sub(token.extension.unwrap().weight)
                .ok_or(ContractError::NegativeValue {})?;

            // Increment the total weight by the weight difference of the new token
            total = total.checked_add(Uint64::from(weight_difference))?;
            // Add the new NFT weight to the old weight for the owner
            new_total_weight = old_total_weight + weight_difference;
            // Set the diff for use in hooks
            diff = MemberDiff::new(owner.owner.clone(), Some(old), Some(new_total_weight));
        }
        Ordering::Less => {
            // Subtract the new token weight from the old token weight
            let weight_difference = token
                .extension
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
        owner.owner.clone(),
        new_total_weight,
    )?;

    // MEMBERS.update(
    //     deps.storage,
    //     &token.owner,
    //     env.block.height,
    //     |old| -> Result<_, ContractError> {
    //         let new_total_weight;
    //         let old_total_weight = old.unwrap_or_default();

    //         // Check if new token weight is great than, less than, or equal to
    //         // the old token weight
    //         match weight.cmp(&token.extension.weight) {
    //             Ordering::Greater => {
    //                 // Subtract the old token weight from the new token weight
    //                 let weight_difference = weight
    //                     .checked_sub(token.extension.weight)
    //                     .ok_or(ContractError::NegativeValue {})?;

    //                 // Increment the total weight by the weight difference of the new token
    //                 total = total.checked_add(Uint64::from(weight_difference))?;
    //                 // Add the new NFT weight to the old weight for the owner
    //                 new_total_weight = old_total_weight + weight_difference;
    //                 // Set the diff for use in hooks
    //                 diff = MemberDiff::new(token.clone().owner, old, Some(new_total_weight));
    //             }
    //             Ordering::Less => {
    //                 // Subtract the new token weight from the old token weight
    //                 let weight_difference = token
    //                     .extension
    //                     .weight
    //                     .checked_sub(weight)
    //                     .ok_or(ContractError::NegativeValue {})?;

    //                 // Subtract the weight difference from the old total weight
    //                 new_total_weight = old_total_weight
    //                     .checked_sub(weight_difference)
    //                     .ok_or(ContractError::NegativeValue {})?;

    //                 // Subtract difference from the total
    //                 total = total.checked_sub(Uint64::from(weight_difference))?;
    //             }
    //             Ordering::Equal => return Err(ContractError::NoWeightChange {}),
    //         }

    //         Ok(new_total_weight)
    //     },
    // )?;
    TotalStore::save(deps.storage, env.block.height, total.u64())?;

    let diffs = MemberChangedHookMsg { diffs: vec![diff] };

    // Prepare hook messages
    let msgs = HOOKS.prepare_hooks(deps.storage, |h| {
        diffs.clone().into_cosmos_msg(h,env.contract.code_hash).map(SubMsg::new)
    })?;

    // Save token weight
    let exec_msg = WasmMsg::Execute {
        contract_addr: snip721_info.contract_address.to_string().clone(),
        code_hash: snip721_info.code_hash.clone(),
        msg: to_binary(&snip721_reference_impl::msg::ExecuteMsg::SetMetadata {
            token_id: token_id.clone(),
            public_metadata: Some(snip721_reference_impl::token::Metadata {
                token_uri: Some(token.token_uri.unwrap()),
                extension: Some(snip721_reference_impl::token::Extension {
                    image: Some(token.extension.clone().unwrap().image.unwrap()),
                    image_data: Some(token.extension.clone().unwrap().image_data.unwrap()),
                    external_url: Some(token.extension.clone().unwrap().external_url.unwrap()),
                    description: Some(token.extension.clone().unwrap().description.unwrap()),
                    name: Some(token.extension.clone().unwrap().name.unwrap()),
                    attributes: Some(token.extension.clone().unwrap().attributes.unwrap()),
                    background_color: Some(token.extension.clone().unwrap().background_color.unwrap()),
                    animation_url: Some(token.extension.clone().unwrap().animation_url.unwrap()),
                    youtube_url: Some(token.extension.clone().unwrap().youtube_url.unwrap()),
                    media: Some(token.extension.clone().unwrap().media.unwrap()),
                    protected_attributes: Some(token.extension.clone().unwrap().protected_attributes.unwrap()),
                    token_subtype: Some(token.extension.clone().unwrap().token_subtype.unwrap()),
                    role: Some(token.extension.unwrap().role.unwrap()),
                    weight,
                }),
            }),
            private_metadata: None,
            padding: None,
        })?,
        funds: vec![],
    };

    Ok(Response::default()
        .add_submessages(msgs)
        .add_attribute("action", "update_token_weight")
        .add_attribute("sender", info.sender)
        .add_attribute("token_id", token_id)
        .add_attribute("weight", weight.to_string())
        .add_message(exec_msg))
}

// #[cfg_attr(not(feature = "library"), entry_point)]
// pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
//     match msg {
//         QueryMsg::Extension { msg } => match msg {
//             QueryExt::Hooks {} => to_binary(&HOOKS.query_hooks(deps)?),
//             QueryExt::ListMembers { start_after, limit } => {
//                 to_binary(&query_list_members(deps, start_after, limit)?)
//             }
//             QueryExt::Member { addr, at_height } => {
//                 to_binary(&query_member(deps, addr, at_height)?)
//             }
//             QueryExt::TotalWeight { at_height } => to_binary(&query_total_weight(deps, at_height)?),
//         },
//         _ => Cw721Roles::default().query(deps, env, msg),
//     }
// }

// pub fn query_total_weight(deps: Deps, height: Option<u64>) -> StdResult<TotalWeightResponse> {
//     let weight = match height {
//         Some(h) => TOTAL.may_load_at_height(deps.storage, h),
//         None => TOTAL.may_load(deps.storage),
//     }?
//     .unwrap_or_default();
//     Ok(TotalWeightResponse { weight })
// }

// pub fn query_member(deps: Deps, addr: String, height: Option<u64>) -> StdResult<MemberResponse> {
//     let addr = deps.api.addr_validate(&addr)?;
//     let weight = match height {
//         Some(h) => MEMBERS.may_load_at_height(deps.storage, &addr, h),
//         None => MEMBERS.may_load(deps.storage, &addr),
//     }?;
//     Ok(MemberResponse { weight })
// }

// pub fn query_list_members(
//     deps: Deps,
//     start_after: Option<String>,
//     limit: Option<u32>,
// ) -> StdResult<MemberListResponse> {
//     let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
//     let addr = maybe_addr(deps.api, start_after)?;
//     let start = addr.as_ref().map(Bound::exclusive);

//     let members = MEMBERS
//         .range(deps.storage, start, None, Order::Ascending)
//         .take(limit)
//         .map(|item| {
//             item.map(|(addr, weight)| Member {
//                 addr: addr.into(),
//                 weight,
//             })
//         })
//         .collect::<StdResult<_>>()?;

//     Ok(MemberListResponse { members })
// }
