use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::snip721::{self, Snip721ExecuteMsg, Snip721QueryMsg};
use cosmwasm_schema::serde;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response,
    StdError, StdResult, SubMsg, SubMsgResult, Uint64, WasmMsg,
};
use cw4::{
    Member, MemberChangedHookMsg, MemberDiff, MemberListResponse, MemberResponse,
    TotalWeightResponse,
};
use schemars::JsonSchema;
use secret_cw_controllers::HookItem;
use secret_toolkit::permit::{Permit, RevokedPermits, TokenPermissions};
use secret_toolkit::utils::InitCallback;
use secret_toolkit::viewing_key::{ViewingKey, ViewingKeyStore};
use serde::{Deserialize, Serialize};
// use cw721_base::Cw721Contract;
// use snip721_reference_impl::msg::InstantiateMsg as Cw721BaseInstantiateMsg;

use dao_snip721_extensions::roles::{
    CreateViewingKey, ExecuteExt, QueryExt, QueryWithPermit, ViewingKeyError,
};
use std::cmp::Ordering;
// use snip721_reference_impl::msg::{ExecuteMsg as Snip721ExecuteMsg};

use crate::state::{Config, MembersStore, TotalStore, MEMBERS_PRIMARY, SNIP721_INFO};
use crate::{error::RolesContractError as ContractError, state::HOOKS};

// Version info for migration
const CONTRACT_NAME: &str = "crates.io:snip721-roles";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Settings for query pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

pub const PREFIX_REVOKED_PERMITS: &str = "revoked_permits";

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
    cw_ownable::initialize_owner(deps.storage, deps.api, Some(&info.sender.to_string()))?;
    // Cw721Roles::default().instantiate(deps.branch(), env.clone(), info, msg)?;

    // init snip721
    let init_msg = snip721::Snip721InstantiateMsg {
        name: msg.name,
        symbol: msg.symbol,
        admin: Some(env.contract.address.to_string().clone()),
        entropy: msg.entropy,
        royalty_info: None,
        config: msg.config,
        post_init_callback: None,
    };
    let submsg = SubMsg::reply_always(
        init_msg.to_cosmos_msg(
            Some(info.sender.clone().to_string()),
            msg.label.clone(),
            msg.code_id.clone(),
            msg.code_hash.clone(),
            None,
        )?,
        SNIP721_INIT_ID,
    );

    // Initialize total weight to zero
    TotalStore::save(deps.storage, env.block.height, 0)?;

    secret_cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
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
        ExecuteMsg::Snip721Execute(snip721_exec_msg) => match snip721_exec_msg {
            Snip721ExecuteMsg::MintNft {
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
                token_id.clone(),
                owner.clone(),
                public_metadata.clone(),
                private_metadata.clone(),
                serial_number.clone(),
                royalty_info.clone(),
                transferable.clone(),
                memo.clone(),
                padding.clone(),
            ),
            Snip721ExecuteMsg::BurnNft {
                token_id,
                memo,
                padding,
            } => execute_burn(
                deps,
                env,
                info,
                token_id.clone(),
                memo.clone(),
                padding.clone(),
            ),
            Snip721ExecuteMsg::TransferNft {
                recipient,
                token_id,
                memo,
                padding,
            } => execute_transfer(
                deps,
                env,
                info,
                recipient.clone(),
                token_id.clone(),
                memo.clone(),
                padding.clone(),
            ),
            Snip721ExecuteMsg::SendNft {
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
                contract.clone(),
                receiver_info.clone(),
                token_id.clone(),
                msg.clone().unwrap(),
                memo.clone(),
                padding.clone(),
            ),
            _ => {
                let snip721_info = SNIP721_INFO.load(deps.storage)?;
                let exec_msg = WasmMsg::Execute {
                    contract_addr: snip721_info.contract_address.to_string(),
                    code_hash: snip721_info.code_hash,
                    msg: to_binary(&snip721_exec_msg)?,
                    funds: vec![],
                };
                Ok(Response::default().add_message(exec_msg))
            }
        },
        ExecuteMsg::ExtensionExecute(extension_msg) => match extension_msg {
            ExecuteExt::AddHook { addr, code_hash } => {
                execute_add_hook(deps, info, addr, code_hash)
            }
            ExecuteExt::RemoveHook { addr, code_hash } => {
                execute_remove_hook(deps, info, addr, code_hash)
            }
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
            ExecuteExt::CreateViewingKey { entropy, .. } => {
                try_create_key(deps, env, info, entropy)
            }
            ExecuteExt::SetViewingKey { key, .. } => try_set_key(deps, info, key),
            ExecuteExt::RevokePermit { permit_name, .. } => revoke_permit(deps, info, permit_name),
        },
    }
}

pub fn execute_mint(
    deps: DepsMut,
    env: &Env,
    _sender: &Addr,
    token_id: Option<String>,
    owner: Option<String>,
    public_metadata: Option<snip721::Metadata>,
    private_metadata: Option<snip721::Metadata>,
    serial_number: Option<snip721::SerialNumber>,
    royalty_info: Option<snip721::RoyaltyInfo>,
    transferable: Option<bool>,
    memo: Option<String>,
    padding: Option<String>,
) -> Result<Response, ContractError> {
    let snip721_info = SNIP721_INFO.load(deps.storage)?;
    let mut total = Uint64::from(TotalStore::load(deps.storage));
    let mut diff = MemberDiff::new(owner.clone().unwrap(), None, None);
    let _ = diff; // reading the value in diff so we don't get warning
    let old = MembersStore::load(
        deps.storage,
        deps.api.addr_validate(&owner.clone().unwrap())?,
    );
    // Increment the total weight by the weight of the new token
    total = total.checked_add(Uint64::from(
        public_metadata.clone().unwrap().extension.unwrap().weight,
    ))?;
    // Add the new NFT weight to the old weight for the owner
    let new_weight = old + public_metadata.clone().unwrap().extension.unwrap().weight;
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
        deps.api.addr_validate(&owner.clone().unwrap())?,
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

    // //add this contract to be minter
    // let minter_msg = WasmMsg::Execute {
    //     contract_addr: snip721_info.contract_address.to_string().clone(),
    //     code_hash: snip721_info.code_hash.clone(),
    //     msg: to_binary(&Snip721ExecuteMsg::AddMinters {
    //         minters: vec![env.contract.address.clone().to_string()],
    //         padding: None,
    //     })?,
    //     funds: vec![],
    // };
    // Call Snip721 mint
    let exec_msg = WasmMsg::Execute {
        contract_addr: snip721_info.contract_address.to_string(),
        code_hash: snip721_info.code_hash.clone(),
        msg: to_binary(&Snip721ExecuteMsg::MintNft {
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
        // .add_message(minter_msg)
        .add_message(exec_msg))
}

pub fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token_id: String,
    _memo: Option<String>,
    _padding: Option<String>,
) -> Result<Response, ContractError> {
    let snip721_info = SNIP721_INFO.load(deps.storage)?;
    // Lookup the owner of the NFT
    let owner: snip721::OwnerOf = deps.querier.query_wasm_smart(
        snip721_info.code_hash.clone(),
        snip721_info.contract_address.to_string().clone(),
        &Snip721QueryMsg::OwnerOf {
            token_id: token_id.clone(),
            viewer: None,
            include_expired: None,
        },
    )?;

    // Get the weight of the token
    let nft_info: snip721::NftInfo = deps.querier.query_wasm_smart(
        snip721_info.code_hash.clone(),
        snip721_info.contract_address.to_string().clone(),
        &Snip721QueryMsg::NftInfo {
            token_id: token_id.clone(),
        },
    )?;

    let mut total = Uint64::from(TotalStore::load(deps.storage));
    let mut diff = MemberDiff::new(owner.owner.clone(), None, None);
    let _ = diff; // reading the value in diff so we don't get warning

    // Update member weights and total
    let owner_addr = owner.owner;
    let old_weight = MembersStore::load(deps.storage, owner_addr.clone());

    // Subtract the nft weight from the member's old weight
    let new_weight = old_weight
        .checked_sub(nft_info.extension.clone().unwrap().weight)
        .ok_or(ContractError::CannotBurn {})?;

    // Subtract nft weight from the total
    total = total.checked_sub(Uint64::from(nft_info.extension.clone().unwrap().weight))?;

    // Check if the new weight is now zero
    if new_weight == 0 {
        // New weight is now None
        diff = MemberDiff::new(owner_addr.clone(), Some(old_weight), None);
        // Remove owner from list of members
        MembersStore::remove(deps.storage, owner_addr.clone())?;
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
        diff = MemberDiff::new(owner_addr.clone(), Some(old), Some(new_weight));
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
        diffs
            .clone()
            .into_cosmos_msg(h.addr, h.code_hash)
            .map(SubMsg::new)
    })?;

    // Burn the token
    let exec_msg = WasmMsg::Execute {
        contract_addr: snip721_info.contract_address.to_string().clone(),
        code_hash: snip721_info.code_hash.clone(),
        msg: to_binary(&Snip721ExecuteMsg::BurnNft {
            token_id: token_id.clone(),
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
    _memo: Option<String>,
    _padding: Option<String>,
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
        msg: to_binary(&Snip721ExecuteMsg::TransferNft {
            recipient: recipient.clone(),
            token_id: token_id.clone(),
            memo: None,
            padding: None,
        })?,
        funds: vec![],
    };

    Ok(Response::new()
        .add_attribute("action", "transfer_nft")
        .add_attribute("sender", info.sender)
        .add_attribute("recipient", recipient.clone())
        .add_attribute("token_id", token_id.clone())
        .add_message(exec_msg))
}

pub fn execute_send(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient_contract: String,
    recipient_info: Option<snip721::ReceiverInfo>,
    token_id: String,
    msg: Binary,
    _memo: Option<String>,
    _padding: Option<String>,
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
        msg: to_binary(&Snip721ExecuteMsg::SendNft {
            contract: recipient_contract.clone(),
            receiver_info: Some(snip721::ReceiverInfo {
                recipient_code_hash: recipient_info.unwrap().recipient_code_hash,
                also_implements_batch_receive_nft: None,
            }),
            token_id: token_id.clone(),
            msg: Some(msg),
            memo: None,
            padding: None,
        })?,
        funds: vec![],
    };

    Ok(Response::new()
        .add_attribute("action", "send_nft")
        .add_attribute("sender", info.sender)
        .add_attribute("recipient", recipient_contract.clone())
        .add_attribute("token_id", token_id.clone())
        .add_message(exec_msg))
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
    let snip721_info = SNIP721_INFO.load(deps.storage)?;
    // Make sure NFT exists
    let token: snip721::NftInfo = deps
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
        msg: to_binary(&Snip721ExecuteMsg::SetMetadata {
            token_id: token_id.clone(),
            public_metadata: Some(snip721::Metadata {
                token_uri: Some(token.token_uri.unwrap()),
                extension: Some(snip721::Extension {
                    image: Some(token.extension.clone().unwrap().image.unwrap()),
                    image_data: Some(token.extension.clone().unwrap().image_data.unwrap()),
                    external_url: Some(token.extension.clone().unwrap().external_url.unwrap()),
                    description: Some(token.extension.clone().unwrap().description.unwrap()),
                    name: Some(token.extension.clone().unwrap().name.unwrap()),
                    attributes: Some(token.extension.clone().unwrap().attributes.unwrap()),
                    background_color: Some(
                        token.extension.clone().unwrap().background_color.unwrap(),
                    ),
                    animation_url: Some(token.extension.clone().unwrap().animation_url.unwrap()),
                    youtube_url: Some(token.extension.clone().unwrap().youtube_url.unwrap()),
                    media: Some(token.extension.clone().unwrap().media.unwrap()),
                    protected_attributes: Some(
                        token
                            .extension
                            .clone()
                            .unwrap()
                            .protected_attributes
                            .unwrap(),
                    ),
                    token_subtype: Some(token.extension.clone().unwrap().token_subtype.unwrap()),
                    role: role.clone(),
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
        .add_attribute("role", role.clone().unwrap_or_default())
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
    let token: snip721::NftInfo = deps
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
        msg: to_binary(&Snip721ExecuteMsg::SetMetadata {
            token_id: token_id.clone(),
            public_metadata: Some(snip721::Metadata {
                token_uri: token_uri.clone(),
                extension: Some(snip721::Extension {
                    image: Some(token.extension.clone().unwrap().image.unwrap()),
                    image_data: Some(token.extension.clone().unwrap().image_data.unwrap()),
                    external_url: Some(token.extension.clone().unwrap().external_url.unwrap()),
                    description: Some(token.extension.clone().unwrap().description.unwrap()),
                    name: Some(token.extension.clone().unwrap().name.unwrap()),
                    attributes: Some(token.extension.clone().unwrap().attributes.unwrap()),
                    background_color: Some(
                        token.extension.clone().unwrap().background_color.unwrap(),
                    ),
                    animation_url: Some(token.extension.clone().unwrap().animation_url.unwrap()),
                    youtube_url: Some(token.extension.clone().unwrap().youtube_url.unwrap()),
                    media: Some(token.extension.clone().unwrap().media.unwrap()),
                    protected_attributes: Some(
                        token
                            .extension
                            .clone()
                            .unwrap()
                            .protected_attributes
                            .unwrap(),
                    ),
                    token_subtype: Some(token.extension.clone().unwrap().token_subtype.unwrap()),
                    role: Some(token.extension.clone().unwrap().role.unwrap()),
                    weight: token.extension.clone().unwrap().weight,
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
        .add_attribute("token_id", token_id.clone())
        .add_attribute("token_uri", token_uri.clone().unwrap_or_default())
        .add_message(exec_msg))
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
    let token: snip721::NftInfo = deps
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
    let owner: snip721::OwnerOf = deps.querier.query_wasm_smart(
        snip721_info.code_hash.clone(),
        snip721_info.contract_address.to_string().clone(),
        &Snip721QueryMsg::OwnerOf {
            token_id: token_id.clone(),
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

    match weight.cmp(&token.extension.clone().unwrap().weight) {
        Ordering::Greater => {
            // Subtract the old token weight from the new token weight
            let weight_difference = weight
                .checked_sub(token.extension.clone().unwrap().weight)
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
        diffs
            .clone()
            .into_cosmos_msg(h.addr, h.code_hash)
            .map(SubMsg::new)
    })?;

    // Save token weight
    let exec_msg = WasmMsg::Execute {
        contract_addr: snip721_info.contract_address.to_string().clone(),
        code_hash: snip721_info.code_hash.clone(),
        msg: to_binary(&Snip721ExecuteMsg::SetMetadata {
            token_id: token_id.clone(),
            public_metadata: Some(snip721::Metadata {
                token_uri: Some(token.token_uri.unwrap()),
                extension: Some(snip721::Extension {
                    image: Some(token.extension.clone().unwrap().image.unwrap()),
                    image_data: Some(token.extension.clone().unwrap().image_data.unwrap()),
                    external_url: Some(token.extension.clone().unwrap().external_url.unwrap()),
                    description: Some(token.extension.clone().unwrap().description.unwrap()),
                    name: Some(token.extension.clone().unwrap().name.unwrap()),
                    attributes: Some(token.extension.clone().unwrap().attributes.unwrap()),
                    background_color: Some(
                        token.extension.clone().unwrap().background_color.unwrap(),
                    ),
                    animation_url: Some(token.extension.clone().unwrap().animation_url.unwrap()),
                    youtube_url: Some(token.extension.clone().unwrap().youtube_url.unwrap()),
                    media: Some(token.extension.clone().unwrap().media.unwrap()),
                    protected_attributes: Some(
                        token
                            .extension
                            .clone()
                            .unwrap()
                            .protected_attributes
                            .unwrap(),
                    ),
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

pub fn try_set_key(
    deps: DepsMut,
    info: MessageInfo,
    key: String,
) -> Result<Response, ContractError> {
    ViewingKey::set(deps.storage, info.sender.as_str(), key.as_str());
    Ok(Response::default())
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

    Ok(Response::new().set_data(to_binary(&CreateViewingKey { key })?))
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
        QueryMsg::ExtensionQuery(extension_query) => match extension_query {
            QueryExt::Hooks {} => to_binary(&HOOKS.query_hooks(deps)?),
            QueryExt::ListMembers { start_after, limit } => {
                to_binary(&query_list_members(deps, start_after, limit)?)
            }
            QueryExt::TotalWeight { at_height } => to_binary(&query_total_weight(deps, at_height)?),
            QueryExt::WithPermit { permit, query } => permit_queries(deps, env, permit, query),
            _ => viewing_keys_queries(deps, env, extension_query),
        },
        QueryMsg::GetNftContractInfo {} => to_binary(&get_info(deps)?),
        _ => {
            let snip721_info = SNIP721_INFO.load(deps.storage)?;
            let res = deps.querier.query_wasm_smart(
                snip721_info.code_hash.clone(),
                snip721_info.contract_address.to_string().clone(),
                &msg,
            )?;
            Ok(to_binary(&res)?)
        }
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
        QueryWithPermit::Member { addr, at_height } => {
            if !permit.check_permission(&TokenPermissions::Balance) {
                return Err(StdError::generic_err(format!(
                    "No permission to query memeber, got permissions {:?}",
                    permit.params.permissions
                )));
            }

            to_binary(&query_member(deps, addr, at_height)?)
        }
    }
}

pub fn viewing_keys_queries(deps: Deps, _env: Env, msg: QueryExt) -> StdResult<Binary> {
    let (addresses, key) = msg.get_validation_params(deps.api)?;

    for address in addresses {
        let result = ViewingKey::check(deps.storage, address.as_str(), key.as_str());
        if result.is_ok() {
            return match msg {
                // Base
                QueryExt::Member {
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

pub fn get_info(deps: Deps) -> StdResult<Config> {
    let res = SNIP721_INFO.load(deps.storage)?;
    Ok(Config {
        contract_address: res.contract_address,
        code_hash: res.code_hash,
    })
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

    let binding = MEMBERS_PRIMARY;
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

#[entry_point]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        SNIP721_INIT_ID => handle_instantiate_reply(deps, msg),
        id => Err(ContractError::UnexpectedReplyId { id }),
    }
}

fn handle_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result {
        SubMsgResult::Ok(res) => {
            let mut snip721_info = SNIP721_INFO.load(deps.storage).unwrap_or_default();
            let data: snip721::InstantiateResponse = from_binary(&res.data.unwrap())?;
            snip721_info.code_hash = data.code_hash;
            snip721_info.contract_address = data.contract_address.to_string();
            SNIP721_INFO.save(deps.storage, &snip721_info)?;
            Ok(Response::new().add_attribute("action", "instantiate"))
        }

        SubMsgResult::Err(e) => Err(ContractError::CustomError { val: e }),
    }
}
