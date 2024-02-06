#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Reply, Response,
    StdResult, SubMsg, SubMsgResult, WasmMsg,
};
use cw4::{MemberResponse, TotalWeightResponse};

use dao_snip721_extensions::roles::QueryExt;
use secret_cw2::set_contract_version;

use crate::msg::{ExecuteMsg, InstantiateMsg, NftContract, QueryMsg};
use crate::state::{Config, CONFIG, DAO, INITIAL_NFTS};
use crate::{error::ContractError, snip721roles};
use secret_toolkit::utils::InitCallback;

pub(crate) const CONTRACT_NAME: &str = "crates.io:dao-voting-snip721-roles";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const INSTANTIATE_NFT_CONTRACT_REPLY_ID: u64 = 0;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<Empty>, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    DAO.save(deps.storage, &info.sender)?;

    match msg.nft_contract {
        NftContract::Existing { address, code_hash } => {
            let config = Config {
                nft_address: deps.api.addr_validate(&address)?,
                nft_code_hash: code_hash.clone(),
            };
            CONFIG.save(deps.storage, &config)?;

            Ok(Response::default()
                .add_attribute("method", "instantiate")
                .add_attribute("nft_contract", address))
        }
        NftContract::New {
            snip721_roles_code_id,
            snip721_roles_code_hash,
            label,
            name,
            symbol,
            initial_nfts,
            entropy,
            config,
            snip721_code_id,
            snip721_code_hash,
        } => {
            // Check there is at least one NFT to initialize
            if initial_nfts.is_empty() {
                return Err(ContractError::NoInitialNfts {});
            }

            // Save initial NFTs for use in reply
            INITIAL_NFTS.save(deps.storage, &initial_nfts)?;

            let init_msg = snip721roles::Snip721RolesInstantiateMsg {
                code_id:snip721_code_id,
                code_hash: snip721_code_hash.clone(),
                label: label.clone(),
                name,
                symbol,
                entropy,
                config,
            };
            // Create instantiate submessage for NFT roles contract
            let msg = SubMsg::reply_on_success(
                init_msg.to_cosmos_msg(
                    Some(info.sender.to_string()),
                    label.clone(),
                    snip721_roles_code_id,
                    snip721_roles_code_hash.clone(),
                    None,
                )?,
                INSTANTIATE_NFT_CONTRACT_REPLY_ID,
            );

            Ok(Response::default().add_submessage(msg))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: ExecuteMsg,
) -> Result<Response<Empty>, ContractError> {
    Err(ContractError::NoExecute {})
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => query_config(deps),
        QueryMsg::Dao {} => query_dao(deps),
        QueryMsg::VotingPowerAtHeight { address, height } => {
            query_voting_power_at_height(deps, env, address, height)
        }
        QueryMsg::TotalPowerAtHeight { height } => query_total_power_at_height(deps, env, height),
        QueryMsg::Info {} => query_info(deps),
    }
}

pub fn query_voting_power_at_height(
    deps: Deps,
    env: Env,
    address: String,
    at_height: Option<u64>,
) -> StdResult<Binary> {
    let config = CONFIG.load(deps.storage)?;
    let member: MemberResponse = deps.querier.query_wasm_smart(
        config.nft_code_hash,
        config.nft_address,
        &snip721_roles::msg::QueryMsg::ExtensionQuery(QueryExt::Member {
            addr: address,
            at_height,
        }),
    )?;

    to_binary(&dao_interface::voting::VotingPowerAtHeightResponse {
        power: member.weight.unwrap_or(0).into(),
        height: at_height.unwrap_or(env.block.height),
    })
}

pub fn query_total_power_at_height(
    deps: Deps,
    env: Env,
    at_height: Option<u64>,
) -> StdResult<Binary> {
    let config = CONFIG.load(deps.storage)?;
    let total: TotalWeightResponse = deps.querier.query_wasm_smart(
        config.nft_code_hash,
        config.nft_address,
        &snip721_roles::msg::QueryMsg::ExtensionQuery(QueryExt::TotalWeight { at_height }),
    )?;

    to_binary(&dao_interface::voting::TotalPowerAtHeightResponse {
        power: total.weight.into(),
        height: at_height.unwrap_or(env.block.height),
    })
}

pub fn query_config(deps: Deps) -> StdResult<Binary> {
    let config = CONFIG.load(deps.storage)?;
    to_binary(&config)
}

pub fn query_dao(deps: Deps) -> StdResult<Binary> {
    let dao = DAO.load(deps.storage)?;
    to_binary(&dao)
}

pub fn query_info(deps: Deps) -> StdResult<Binary> {
    let info = secret_cw2::get_contract_version(deps.storage)?;
    to_binary(&dao_interface::voting::InfoResponse { info })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_NFT_CONTRACT_REPLY_ID => {
            match msg.result {
                SubMsgResult::Ok(res) => {
                    let dao = DAO.load(deps.storage)?;
                    let data: snip721_roles::msg::InstantiateResponse =
                        from_binary(&res.data.unwrap())?;

                    // Save config
                    let config = Config {
                        nft_address: data.contract_address.clone(),
                        nft_code_hash: data.code_hash.clone(),
                    };
                    CONFIG.save(deps.storage, &config)?;

                    let initial_nfts = INITIAL_NFTS.load(deps.storage)?;

                    // Add mint submessages
                    let mint_submessages: Vec<SubMsg> = initial_nfts
                        .iter()
                        .flat_map(|nft| -> Result<SubMsg, ContractError> {
                            Ok(SubMsg::new(WasmMsg::Execute {
                                contract_addr: data.contract_address.to_string().clone(),
                                code_hash: data.code_hash.to_string().clone(),
                                funds: vec![],
                                msg: to_binary(&snip721_roles::msg::ExecuteMsg::Snip721Execute(
                                    snip721_roles::snip721::Snip721ExecuteMsg::MintNft {
                                        token_id: Some(nft.token_id.clone()),
                                        owner: Some(nft.owner.clone()),
                                        public_metadata: Some(snip721_roles::snip721::Metadata {
                                            token_uri: Some(nft.token_uri.clone().unwrap()),
                                            extension: Some(snip721_roles::snip721::Extension {
                                                image: None,
                                                image_data: None,
                                                external_url: None,
                                                description: None,
                                                name: None,
                                                attributes: None,
                                                background_color: None,
                                                animation_url: None,
                                                youtube_url: None,
                                                media: None,
                                                protected_attributes: None,
                                                token_subtype: None,
                                                role: Some(nft.extension.role.clone().unwrap()),
                                                weight: nft.extension.weight,
                                            }),
                                        }),
                                        private_metadata: None,
                                        serial_number: None,
                                        royalty_info: None,
                                        transferable: None,
                                        memo: None,
                                        padding: None,
                                    },
                                ))?,
                            }))
                        })
                        .collect::<Vec<SubMsg>>();

                    // Clear space
                    INITIAL_NFTS.remove(deps.storage);

                    // Update minter message
                    let update_minter_msg = WasmMsg::Execute {
                        contract_addr: data.contract_address.to_string().clone(),
                        code_hash: data.code_hash.to_string().clone(),
                        msg: to_binary(&snip721_roles::msg::ExecuteMsg::Snip721Execute(
                            snip721_roles::snip721::Snip721ExecuteMsg::ChangeAdmin {
                                address: dao.to_string(),
                                padding: None,
                            },
                        ))?,
                        funds: vec![],
                    };

                    Ok(Response::default()
                        .add_attribute("method", "instantiate")
                        .add_attribute("nft_contract", data.contract_address.to_string().clone())
                        .add_message(update_minter_msg)
                        .add_submessages(mint_submessages))
                }
                SubMsgResult::Err(_) => Err(ContractError::NftInstantiateError {}),
            }
        }
        _ => Err(ContractError::UnknownReplyId { id: msg.id }),
    }
}
