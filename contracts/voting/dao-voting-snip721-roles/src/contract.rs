#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Reply, Response,
    StdResult, SubMsg, SubMsgResult, WasmMsg,
};
use cw4::{MemberResponse, TotalWeightResponse};

use dao_interface::state::AnyContractInfo;
use dao_snip721_extensions::roles::{ExecuteExt, MetadataExt, QueryExt};
use secret_cw2::set_contract_version;
use shade_protocol::basic_staking::Auth;

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
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<Empty>, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    DAO.save(
        deps.storage,
        &AnyContractInfo {
            addr: info.sender.clone(),
            code_hash: msg.dao_code_hash,
        },
    )?;

    match msg.nft_contract {
        NftContract::Existing { address, code_hash } => {
            let config = Config {
                nft_address: deps.api.addr_validate(&address)?,
                nft_code_hash: code_hash.clone(),
            };
            CONFIG.save(deps.storage, &config)?;

            Ok(Response::default()
                .add_attribute("method", "instantiate")
                .set_data(to_binary(&AnyContractInfo {
                    addr: env.contract.address,
                    code_hash: env.contract.code_hash,
                })?)
                .add_attribute("nft_contract", address))
        }
        NftContract::New {
            snip721_roles_code_id,
            snip721_roles_code_hash,
            name,
            symbol,
            entropy,
            config,
            query_auth,
            admin,
            royalty_info,
            post_init_callback,
            initial_nfts,
        } => {
            // Check there is at least one NFT to initialize
            if initial_nfts.is_empty() {
                return Err(ContractError::NoInitialNfts {});
            }

            // Save initial NFTs for use in reply
            INITIAL_NFTS.save(deps.storage, &initial_nfts)?;

            let init_msg = snip721roles::Snip721RolesInstantiateMsg {
                name,
                symbol,
                entropy,
                config,
                query_auth,
                admin,
                royalty_info,
                post_init_callback,
            };
            // Create instantiate submessage for NFT roles contract
            let submsg = SubMsg::reply_on_success(
                init_msg.to_cosmos_msg(
                    Some(info.sender.to_string()),
                    env.contract.address.to_string(),
                    snip721_roles_code_id,
                    snip721_roles_code_hash.clone(),
                    None,
                )?,
                INSTANTIATE_NFT_CONTRACT_REPLY_ID,
            );
            let config = Config {
                nft_address: Addr::unchecked(""),
                nft_code_hash: snip721_roles_code_hash.clone(),
            };
            CONFIG.save(deps.storage, &config)?;

            Ok(Response::default()
                .set_data(to_binary(&AnyContractInfo {
                    addr: env.contract.address,
                    code_hash: env.contract.code_hash,
                })?)
                .add_submessage(submsg))
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
        QueryMsg::VotingPowerAtHeight { auth, height } => {
            query_voting_power_at_height(deps, env, auth, height)
        }
        QueryMsg::TotalPowerAtHeight { height } => query_total_power_at_height(deps, env, height),
        QueryMsg::Info {} => query_info(deps),
    }
}

pub fn query_voting_power_at_height(
    deps: Deps,
    env: Env,
    auth: Auth,
    at_height: Option<u64>,
) -> StdResult<Binary> {
    let config = CONFIG.load(deps.storage)?;
    let member: MemberResponse = deps.querier.query_wasm_smart(
        config.nft_code_hash,
        config.nft_address,
        &snip721_roles_impl::msg::QueryMsg::<QueryExt>::QueryExtension {
            msg: QueryExt::Member { auth, at_height },
        },
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
        &snip721_roles_impl::msg::QueryMsg::<QueryExt>::QueryExtension {
            msg: QueryExt::TotalWeight { at_height },
        },
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
                    let mut config = CONFIG.load(deps.storage)?;
                    let nft_roles_info: AnyContractInfo =
                        from_binary(&res.data.clone().unwrap_or_default())?;

                    // Save config
                    config.nft_address = nft_roles_info.addr.clone();

                    let initial_nfts = INITIAL_NFTS.load(deps.storage)?;

                    // Add mint submessages
                    let mint_messages: Vec<WasmMsg> = initial_nfts
                        .iter()
                        .flat_map(|nft| -> Result<WasmMsg, ContractError> {
                            Ok(WasmMsg::Execute {
                                contract_addr: nft_roles_info.addr.clone().to_string(),
                                code_hash: config.nft_code_hash.clone(),
                                msg: to_binary(&snip721_roles_impl::msg::ExecuteMsg::<
                                    MetadataExt,
                                    ExecuteExt,
                                >::MintNft {
                                    token_id: Some(nft.token_id.clone()),
                                    owner: Some(nft.owner.clone()),
                                    public_metadata: None,
                                    private_metadata: None,
                                    serial_number: None,
                                    royalty_info: None,
                                    transferable: None,
                                    memo: None,
                                    padding: None,
                                    extension: MetadataExt {
                                        role: nft.clone().extension.role,
                                        weight: nft.extension.weight,
                                    },
                                })?,
                                funds: vec![],
                            })
                        })
                        .collect::<Vec<WasmMsg>>();

                    // Clear space
                    INITIAL_NFTS.remove(deps.storage);

                    // Update minter message
                    let update_minter_msg = WasmMsg::Execute {
                        contract_addr: nft_roles_info.addr.clone().to_string(),
                        code_hash: config.nft_code_hash.clone(),
                        msg: to_binary(&snip721_roles_impl::msg::ExecuteMsg::<
                            MetadataExt,
                            ExecuteExt,
                        >::ChangeAdmin {
                            address: dao.addr.to_string(),
                            padding: None,
                        })?,
                        funds: vec![],
                    };

                    CONFIG.save(deps.storage, &config)?;

                    Ok(Response::default()
                        .add_attribute("method", "instantiate")
                        .add_attribute("nft_contract", nft_roles_info.addr)
                        .add_message(update_minter_msg)
                        .add_messages(mint_messages))
                }
                SubMsgResult::Err(_) => Err(ContractError::NftInstantiateError {}),
            }
        }
        _ => Err(ContractError::UnknownReplyId { id: msg.id }),
    }
}
