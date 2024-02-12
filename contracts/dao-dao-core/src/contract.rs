#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Empty, Env, MessageInfo, Reply,
    Response, StdError, StdResult, SubMsg,
};
// use cw_paginate_storage::{paginate_map, paginate_map_keys, paginate_map_values};
use dao_interface::{
    msg::{
        ExecuteMsg, InitialItem, InstantiateMsg, MigrateMsg, QueryMsg, Snip20ReceiveMsg,
        Snip721ReceiveMsg,
    },
    query::{
        AdminNominationResponse, DaoURIResponse, DumpStateResponse, GetItemResponse,
        PauseInfoResponse, ProposalModuleCountResponse, Snip20BalanceResponse, SubDao,
    },
    state::{
        Config, ModuleInstantiateCallback, ModuleInstantiateInfo, ProposalModule,
        ProposalModuleStatus, VotingModuleInfo,
    },
    voting,
};
use secret_cw2::{get_contract_version, set_contract_version, ContractVersion};
use secret_toolkit::{storage::Keymap, utils::HandleCallback, utils::InitCallback};
use secret_utils::{parse_reply_instantiate_data, Duration};

use crate::state::{
    ACTIVE_PROPOSAL_MODULE_COUNT, ADMIN, CONFIG, ITEMS, NOMINATED_ADMIN, PAUSED, PROPOSAL_MODULES,
    SNIP20_LIST, SNIP721_LIST, SUBDAO_LIST, TOKEN_VIEWING_KEY, TOTAL_PROPOSAL_MODULE_COUNT,
    VOTING_MODULE,
};
use crate::{error::ContractError, snip20_msg};

pub(crate) const CONTRACT_NAME: &str = "crates.io:dao-dao-core";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const PROPOSAL_MODULE_REPLY_ID: u64 = 0;
const VOTE_MODULE_INSTANTIATE_REPLY_ID: u64 = 1;
const VOTE_MODULE_UPDATE_REPLY_ID: u64 = 2;
const SNIP20_VIEWING_KEY_REPLY_ID: u64 = 3;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        name: msg.clone().name,
        description: msg.clone().description,
        image_url: msg.clone().image_url,
        automatically_add_snip20s: msg.clone().automatically_add_snip20s,
        automatically_add_snip721s: msg.clone().automatically_add_snip721s,
        dao_uri: msg.clone().dao_uri,
    };
    CONFIG.save(deps.storage, &config)?;

    let admin = msg
        .clone()
        .admin
        .map(|human| deps.api.addr_validate(&human))
        .transpose()?
        // If no admin is specified, the contract is its own admin.
        .unwrap_or_else(|| env.contract.address.clone());
    ADMIN.save(deps.storage, &admin)?;

    // Adding code hash here only as we don't know what voting module we are using as we are providing it in binary
    let voting_code_hash = msg.clone().voting_module_instantiate_info.code_hash;
    let voting_module_info = VotingModuleInfo {
        addr: Addr::unchecked(""),
        code_hash: voting_code_hash,
    };
    VOTING_MODULE.save(deps.storage, &voting_module_info)?;
    let vote_module_msg = msg.clone().voting_module_instantiate_info.to_cosmos_msg(
        Some(env.contract.address.clone().to_string()),
        env.contract.address.clone().to_string(),
        msg.clone().voting_module_instantiate_info.code_id,
        msg.clone().voting_module_instantiate_info.code_hash,
        None,
    )?;
    let vote_module_msg: SubMsg<Empty> =
        SubMsg::reply_on_success(vote_module_msg, VOTE_MODULE_INSTANTIATE_REPLY_ID);

    let proposal_module_msgs: Vec<SubMsg<Empty>> = msg
        .proposal_modules_instantiate_info
        .into_iter()
        .map(|info| {
            info.clone()
                .to_cosmos_msg(
                    Some(env.contract.address.clone().to_string()),
                    env.contract.address.clone().to_string(),
                    info.clone().code_id,
                    info.clone().code_hash,
                    None,
                )
                .unwrap()
        })
        .map(|wasm| SubMsg::reply_on_success(wasm, PROPOSAL_MODULE_REPLY_ID))
        .collect();
    if proposal_module_msgs.is_empty() {
        return Err(ContractError::NoActiveProposalModules {});
    }

    if let Some(initial_items) = msg.initial_items {
        // O(N*N) deduplication.
        let mut seen = Vec::with_capacity(initial_items.len());
        for InitialItem { key, value } in initial_items {
            if seen.contains(&key) {
                return Err(ContractError::DuplicateInitialItem { item: key });
            }
            seen.push(key.clone());
            ITEMS.insert(deps.storage, &key, &value)?;
        }
    }

    TOTAL_PROPOSAL_MODULE_COUNT.save(deps.storage, &0)?;
    ACTIVE_PROPOSAL_MODULE_COUNT.save(deps.storage, &0)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("sender", info.sender)
        .add_submessage(vote_module_msg)
        .add_submessages(proposal_module_msgs))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    // No actions can be performed while the DAO is paused.
    if let Some(expiration) = PAUSED.may_load(deps.storage)? {
        if !expiration.is_expired(&env.block) {
            return Err(ContractError::Paused {});
        }
    }

    match msg {
        ExecuteMsg::ExecuteAdminMsgs { msgs } => {
            execute_admin_msgs(deps.as_ref(), info.sender, msgs)
        }
        ExecuteMsg::ExecuteProposalHook { msgs } => {
            execute_proposal_hook(deps.as_ref(), info.sender, msgs)
        }
        ExecuteMsg::Pause { duration } => execute_pause(deps, env, info.sender, duration),
        ExecuteMsg::Receive(msg) => execute_receive_snip20(deps, info.sender, msg),
        ExecuteMsg::ReceiveNft(msg) => execute_receive_snip721(deps, info.sender, msg),
        ExecuteMsg::RemoveItem { key } => execute_remove_item(deps, env, info.sender, key),
        ExecuteMsg::SetItem { key, value } => execute_set_item(deps, env, info.sender, key, value),
        ExecuteMsg::UpdateConfig { config } => {
            execute_update_config(deps, env, info.sender, config)
        }
        ExecuteMsg::UpdateSnip20List { to_add, to_remove } => {
            execute_update_snip20_list(deps, env, info.sender, to_add, to_remove)
        }
        ExecuteMsg::UpdateSnip721List { to_add, to_remove } => {
            execute_update_snip721_list(deps, env, info.sender, to_add, to_remove)
        }
        ExecuteMsg::UpdateVotingModule { module } => {
            execute_update_voting_module(deps, env, info.sender, module)
        }
        ExecuteMsg::UpdateProposalModules { to_add, to_disable } => {
            execute_update_proposal_modules(deps, env, info.sender, to_add, to_disable)
        }
        ExecuteMsg::NominateAdmin { admin } => {
            execute_nominate_admin(deps, env, info.sender, admin)
        }
        ExecuteMsg::AcceptAdminNomination {} => execute_accept_admin_nomination(deps, info.sender),
        ExecuteMsg::WithdrawAdminNomination {} => {
            execute_withdraw_admin_nomination(deps, info.sender)
        }
        ExecuteMsg::UpdateSubDaos { to_add, to_remove } => {
            execute_update_sub_daos_list(deps, env, info.sender, to_add, to_remove)
        }
    }
}

pub fn execute_pause(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    pause_duration: Duration,
) -> Result<Response, ContractError> {
    // Only the core contract may call this method.
    if sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    let until = pause_duration.after(&env.block);

    PAUSED.save(deps.storage, &until)?;

    Ok(Response::new()
        .add_attribute("action", "execute_pause")
        .add_attribute("sender", sender)
        .add_attribute("until", until.to_string()))
}

pub fn execute_admin_msgs(
    deps: Deps,
    sender: Addr,
    msgs: Vec<CosmosMsg<Empty>>,
) -> Result<Response, ContractError> {
    let admin = ADMIN.load(deps.storage)?;

    // Check if the sender is the DAO Admin
    if sender != admin {
        return Err(ContractError::Unauthorized {});
    }

    Ok(Response::default()
        .add_attribute("action", "execute_admin_msgs")
        .add_messages(msgs))
}

pub fn execute_proposal_hook(
    deps: Deps,
    sender: Addr,
    msgs: Vec<CosmosMsg<Empty>>,
) -> Result<Response, ContractError> {
    let module = PROPOSAL_MODULES
        .get(deps.storage, &sender.clone())
        .ok_or(ContractError::Unauthorized {})?;

    // Check that the message has come from an active module
    if module.status != ProposalModuleStatus::Enabled {
        return Err(ContractError::ModuleDisabledCannotExecute { address: sender });
    }

    Ok(Response::default()
        .add_attribute("action", "execute_proposal_hook")
        .add_messages(msgs))
}

pub fn execute_nominate_admin(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    nomination: Option<String>,
) -> Result<Response, ContractError> {
    let nomination = nomination.map(|h| deps.api.addr_validate(&h)).transpose()?;

    let current_admin = ADMIN.load(deps.storage)?;
    if current_admin != sender {
        return Err(ContractError::Unauthorized {});
    }

    let current_nomination = NOMINATED_ADMIN.may_load(deps.storage)?;
    if current_nomination.is_some() {
        return Err(ContractError::PendingNomination {});
    }

    match &nomination {
        Some(nomination) => NOMINATED_ADMIN.save(deps.storage, nomination)?,
        // If no admin set to default of the contract. This allows the
        // contract to later set a new admin via governance.
        None => ADMIN.save(deps.storage, &env.contract.address)?,
    }

    Ok(Response::default()
        .add_attribute("action", "execute_nominate_admin")
        .add_attribute(
            "nomination",
            nomination
                .map(|n| n.to_string())
                .unwrap_or_else(|| "None".to_string()),
        ))
}

pub fn execute_accept_admin_nomination(
    deps: DepsMut,
    sender: Addr,
) -> Result<Response, ContractError> {
    let nomination = NOMINATED_ADMIN
        .may_load(deps.storage)?
        .ok_or(ContractError::NoAdminNomination {})?;
    if sender != nomination {
        return Err(ContractError::Unauthorized {});
    }
    NOMINATED_ADMIN.remove(deps.storage);
    ADMIN.save(deps.storage, &nomination)?;

    Ok(Response::default()
        .add_attribute("action", "execute_accept_admin_nomination")
        .add_attribute("new_admin", sender))
}

pub fn execute_withdraw_admin_nomination(
    deps: DepsMut,
    sender: Addr,
) -> Result<Response, ContractError> {
    let admin = ADMIN.load(deps.storage)?;
    if admin != sender {
        return Err(ContractError::Unauthorized {});
    }

    // Check that there is indeed a nomination to withdraw.
    let current_nomination = NOMINATED_ADMIN.may_load(deps.storage)?;
    if current_nomination.is_none() {
        return Err(ContractError::NoAdminNomination {});
    }

    NOMINATED_ADMIN.remove(deps.storage);

    Ok(Response::default()
        .add_attribute("action", "execute_withdraw_admin_nomination")
        .add_attribute("sender", sender))
}

pub fn execute_update_config(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    config: Config,
) -> Result<Response, ContractError> {
    if sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    CONFIG.save(deps.storage, &config)?;
    // We incur some gas costs by having the config's fields in the
    // response. This has the benefit that it makes it reasonably
    // simple to ask "when did this field in the config change" by
    // running something like `junod query txs --events
    // 'wasm._contract_address=core&wasm.name=name'`.
    Ok(Response::default()
        .add_attribute("action", "execute_update_config")
        .add_attribute("name", config.name)
        .add_attribute("description", config.description)
        .add_attribute(
            "image_url",
            config.image_url.unwrap_or_else(|| "None".to_string()),
        ))
}

pub fn execute_update_voting_module(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    module: ModuleInstantiateInfo,
) -> Result<Response, ContractError> {
    if env.contract.address != sender {
        return Err(ContractError::Unauthorized {});
    }
    let mut current = VOTING_MODULE.load(deps.storage)?;
    current.code_hash = module.clone().code_hash;
    VOTING_MODULE.save(deps.storage, &current)?;

    let wasm = module.clone().to_cosmos_msg(
        Some(env.contract.address.clone().to_string()),
        env.contract.address.to_string(),
        module.clone().code_id,
        module.clone().code_hash,
        None,
    )?;
    let submessage = SubMsg::reply_on_success(wasm, VOTE_MODULE_UPDATE_REPLY_ID);

    Ok(Response::default()
        .add_attribute("action", "execute_update_voting_module")
        .add_submessage(submessage))
}

pub fn execute_update_proposal_modules(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    to_add: Vec<ModuleInstantiateInfo>,
    to_disable: Vec<String>,
) -> Result<Response, ContractError> {
    if env.contract.address != sender {
        return Err(ContractError::Unauthorized {});
    }

    let disable_count = to_disable.len() as u32;
    for addr in to_disable {
        let addr = deps.api.addr_validate(&addr)?;
        let mut module = PROPOSAL_MODULES.get(deps.storage, &addr.clone()).ok_or(
            ContractError::ProposalModuleDoesNotExist {
                address: addr.clone(),
            },
        )?;

        if module.status == ProposalModuleStatus::Disabled {
            return Err(ContractError::ModuleAlreadyDisabled {
                address: module.address,
            });
        }

        module.status = ProposalModuleStatus::Disabled {};
        PROPOSAL_MODULES.insert(deps.storage, &addr, &module)?;
    }

    // If disabling this module will cause there to be no active modules, return error.
    // We don't check the active count before disabling because there may erroneously be
    // modules in to_disable which are already disabled.
    ACTIVE_PROPOSAL_MODULE_COUNT.update(deps.storage, |count| {
        if count <= disable_count && to_add.is_empty() {
            return Err(ContractError::NoActiveProposalModules {});
        }
        Ok(count - disable_count)
    })?;

    let to_add: Vec<SubMsg<Empty>> = to_add
        .into_iter()
        .map(|info| info.into_wasm_msg(env.contract.address.clone()))
        .map(|wasm| SubMsg::reply_on_success(wasm, PROPOSAL_MODULE_REPLY_ID))
        .collect();

    Ok(Response::default()
        .add_attribute("action", "execute_update_proposal_modules")
        .add_submessages(to_add))
}

/// Updates a set of addresses in state applying VERIFY to each item
/// that will be added.
fn do_update_addr_list(
    deps: DepsMut,
    map: &Keymap<(Addr, String), Empty>,
    to_add: Vec<(String, String)>, // with code hashes
    // to_add_code_hashes: Vec<String>,
    to_remove: Vec<(String, String)>,
    verify: impl Fn(&Addr, &String, Deps) -> StdResult<()>,
) -> Result<(), ContractError> {
    // let to_add = to_add
    //     .into_iter()
    //     .map(|(a,c)| deps.api.addr_validate(&a))
    //     .collect::<Result<Vec<(_,_)>, _>>()?;

    // let to_remove = to_remove
    //     .into_iter()
    //     .map(|a| deps.api.addr_validate(&a))
    //     .collect::<Result<Vec<_>, _>>()?;

    for (addr, code_hash) in to_add {
        verify(&deps.api.addr_validate(&addr)?, &code_hash, deps.as_ref())?;
        map.insert(
            deps.storage,
            &(deps.api.addr_validate(&addr)?, code_hash),
            &Empty {},
        )?;
    }
    for (addr, code_hash) in to_remove {
        map.remove(deps.storage, &(deps.api.addr_validate(&addr)?, code_hash))?;
    }

    Ok(())
}

pub fn execute_update_snip20_list(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    to_add: Vec<(String, String)>,
    to_remove: Vec<(String, String)>,
) -> Result<Response, ContractError> {
    if env.contract.address != sender {
        return Err(ContractError::Unauthorized {});
    }
    do_update_addr_list(
        deps,
        &SNIP20_LIST,
        to_add,
        to_remove,
        |addr, code_hash, deps| {
            // Perform a balance query here as this is the query performed
            // by the `Cw20Balances` query.
            let viewing_key = TOKEN_VIEWING_KEY.load(deps.storage).unwrap_or_default();
            let _info: snip20_reference_impl::msg::Balance = deps.querier.query_wasm_smart(
                code_hash,
                addr,
                &snip20_reference_impl::msg::QueryMsg::Balance {
                    address: env.contract.address.to_string(),
                    key: viewing_key,
                },
            )?;
            Ok(())
        },
    )?;
    Ok(Response::default().add_attribute("action", "update_snip20_list"))
}

pub fn execute_update_snip721_list(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    to_add: Vec<(String, String)>,
    to_remove: Vec<(String, String)>,
) -> Result<Response, ContractError> {
    if env.contract.address != sender {
        return Err(ContractError::Unauthorized {});
    }
    do_update_addr_list(
        deps,
        &SNIP721_LIST,
        to_add,
        to_remove,
        |addr, code_hash, deps| {
            let _info: snip721_reference_impl::msg::ContractInfo = deps.querier.query_wasm_smart(
                code_hash,
                addr,
                &snip721_reference_impl::msg::QueryMsg::ContractInfo {},
            )?;
            Ok(())
        },
    )?;
    Ok(Response::default().add_attribute("action", "update_cw721_list"))
}

pub fn execute_set_item(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    key: String,
    value: String,
) -> Result<Response, ContractError> {
    if env.contract.address != sender {
        return Err(ContractError::Unauthorized {});
    }

    ITEMS.insert(deps.storage, &key.clone(), &value)?;
    Ok(Response::default()
        .add_attribute("action", "execute_set_item")
        .add_attribute("key", key)
        .add_attribute("addr", value))
}

pub fn execute_remove_item(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    key: String,
) -> Result<Response, ContractError> {
    if env.contract.address != sender {
        return Err(ContractError::Unauthorized {});
    }

    if ITEMS.get(deps.storage, &key.clone()).is_some() {
        ITEMS.remove(deps.storage, &key.clone())?;
        Ok(Response::default()
            .add_attribute("action", "execute_remove_item")
            .add_attribute("key", key))
    } else {
        Err(ContractError::KeyMissing {})
    }
}

pub fn execute_update_sub_daos_list(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    to_add: Vec<SubDao>,
    to_remove: Vec<String>,
) -> Result<Response, ContractError> {
    if env.contract.address != sender {
        return Err(ContractError::Unauthorized {});
    }

    for addr in to_remove {
        let addr = deps.api.addr_validate(&addr)?;
        SUBDAO_LIST.remove(deps.storage, &addr)?;
    }

    for subdao in to_add {
        let addr = deps.api.addr_validate(&subdao.addr)?;
        SUBDAO_LIST.insert(deps.storage, &addr, &subdao.charter)?;
    }

    Ok(Response::default()
        .add_attribute("action", "execute_update_sub_daos_list")
        .add_attribute("sender", sender))
}

pub fn execute_receive_snip20(
    deps: DepsMut,
    sender: Addr,
    wrapper: Snip20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if !config.automatically_add_snip20s {
        Ok(Response::new())
    } else {
        // Create Snip20 Token viewing key
        let gen_viewing_key_msg = snip20_msg::Snip20ExecuteMsg::CreateViewingKey {
            entropy: "entropy".to_string(),
            padding: None,
        };
        let submsg = SubMsg::reply_always(
            gen_viewing_key_msg.to_cosmos_msg(
                wrapper.code_hash.clone(),
                sender.clone().to_string(),
                None,
            )?,
            SNIP20_VIEWING_KEY_REPLY_ID,
        );
        SNIP20_LIST.insert(
            deps.storage,
            &(sender.clone(), wrapper.code_hash),
            &Empty {},
        )?;
        Ok(Response::new()
            .add_attribute("action", "receive_snip20")
            .add_attribute("token", sender)
            .add_submessage(submsg))
    }
}

pub fn execute_receive_snip721(
    deps: DepsMut,
    sender: Addr,
    wrapper: Snip721ReceiveMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if !config.automatically_add_snip721s {
        Ok(Response::new())
    } else {
        if let Snip721ReceiveMsg::ReceiveNft {
            sender,
            code_hash,
            token_id: _,
            msg: _,
        } = wrapper
        {
            SNIP721_LIST.insert(deps.storage, &(sender.clone(), code_hash), &Empty {})?;
        }
        Ok(Response::new()
            .add_attribute("action", "receive_cw721")
            .add_attribute("token", sender))
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Admin {} => query_admin(deps),
        QueryMsg::AdminNomination {} => query_admin_nomination(deps),
        QueryMsg::Config {} => query_config(deps),
        QueryMsg::Cw20TokenList { start_after, limit } => query_cw20_list(deps, start_after, limit),
        QueryMsg::Cw20Balances { start_after, limit } => {
            query_cw20_balances(deps, env, start_after, limit)
        }
        QueryMsg::Cw721TokenList { start_after, limit } => {
            query_cw721_list(deps, start_after, limit)
        }
        QueryMsg::DumpState {} => query_dump_state(deps, env),
        QueryMsg::GetItem { key } => query_get_item(deps, key),
        QueryMsg::Info {} => query_info(deps),
        QueryMsg::ListItems { start_after, limit } => query_list_items(deps, start_after, limit),
        QueryMsg::PauseInfo {} => query_paused(deps, env),
        QueryMsg::ProposalModules { start_after, limit } => {
            query_proposal_modules(deps, start_after, limit)
        }
        QueryMsg::ProposalModuleCount {} => query_proposal_module_count(deps),
        QueryMsg::TotalPowerAtHeight { height } => query_total_power_at_height(deps, height),
        QueryMsg::VotingModule {} => query_voting_module(deps),
        QueryMsg::VotingPowerAtHeight { address, height } => {
            query_voting_power_at_height(deps, address, height)
        }
        QueryMsg::ActiveProposalModules { start_after, limit } => {
            query_active_proposal_modules(deps, start_after, limit)
        }
        QueryMsg::ListSubDaos { start_after, limit } => {
            query_list_sub_daos(deps, start_after, limit)
        }
        QueryMsg::DaoURI {} => query_dao_uri(deps),
    }
}

pub fn query_admin(deps: Deps) -> StdResult<Binary> {
    let admin = ADMIN.load(deps.storage)?;
    to_binary(&admin)
}

pub fn query_admin_nomination(deps: Deps) -> StdResult<Binary> {
    let nomination = NOMINATED_ADMIN.may_load(deps.storage)?;
    to_binary(&AdminNominationResponse { nomination })
}

pub fn query_config(deps: Deps) -> StdResult<Binary> {
    let config = CONFIG.load(deps.storage)?;
    to_binary(&config)
}

pub fn query_voting_module(deps: Deps) -> StdResult<Binary> {
    let voting_module = VOTING_MODULE.load(deps.storage)?;
    to_binary(&voting_module)
}

pub fn query_proposal_modules(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Binary> {
    // This query is will run out of gas due to the size of the
    // returned message before it runs out of compute so taking a
    // limit here is still nice. As removes happen in constant time
    // the contract is still recoverable if too many items end up in
    // here.
    //
    // Further, as the `range` method on a map returns an iterator it
    // is possible (though implementation dependent) that new keys are
    // loaded on demand as the iterator is moved. Should this be the
    // case we are only paying for what we need here.
    //
    // Even if this does lock up one can determine the existing
    // proposal modules by looking at past transactions on chain.
   
    // let data = paginate_map_values(
    //     deps,
    //     &PROPOSAL_MODULES,
    //     0,
    //     PROPOSAL_MODULES.get_len(deps.storage).unwrap_or_default(),
    // )?;

    let mut res:Vec<ProposalModule>=Vec::new();
    let mut start=start_after.clone();
    let binding = &PROPOSAL_MODULES;
    let iter = binding.iter(deps.storage)?;
    for item in iter {
        let (address, module) = item?;
        if let Some(start_after) = &start {
            if &address == start_after {
                // If we found the start point, reset it to start iterating
                start = None;
            }
        }
        if start.is_none() {
            res.push(ProposalModule { address: module.clone().address, prefix: module.clone().prefix, status: module.clone().status });
            if res.len() >= limit.unwrap() as usize {
                break; // Break out of loop if limit reached
            }
        }
    }
    to_binary(&res)
}

pub fn query_active_proposal_modules(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Binary> {
    // Note: this is not gas efficient as we need to potentially visit all modules in order to
    // filter out the modules with active status.
    let mut res:Vec<ProposalModule>=Vec::new();
    let mut start=start_after.clone();
    let binding = &PROPOSAL_MODULES;
    let iter = binding.iter(deps.storage)?;
    for item in iter {
        let (address, module) = item?;
        if let Some(start_after) = &start {
            if &address == start_after {
                // If we found the start point, reset it to start iterating
                start = None;
            }
        }
        if start.is_none() {
            res.push(ProposalModule { address: module.clone().address, prefix: module.clone().prefix, status: module.clone().status });
            if res.len() >= limit.unwrap() as usize {
                break; // Break out of loop if limit reached
            }
        }
    }

    let limit = limit.unwrap_or(res.len() as u32);

    to_binary::<Vec<ProposalModule>>(
        &res
            .into_iter()
            .filter(|module: &ProposalModule| module.status == ProposalModuleStatus::Enabled)
            .take(limit as usize)
            .collect(),
    )
}

fn get_pause_info(deps: Deps, env: Env) -> StdResult<PauseInfoResponse> {
    Ok(match PAUSED.may_load(deps.storage)? {
        Some(expiration) => {
            if expiration.is_expired(&env.block) {
                PauseInfoResponse::Unpaused {}
            } else {
                PauseInfoResponse::Paused { expiration }
            }
        }
        None => PauseInfoResponse::Unpaused {},
    })
}

pub fn query_paused(deps: Deps, env: Env) -> StdResult<Binary> {
    to_binary(&get_pause_info(deps, env)?)
}

pub fn query_dump_state(deps: Deps, env: Env) -> StdResult<Binary> {
    let admin = ADMIN.load(deps.storage)?;
    let mut proposal_module_res: Vec<ProposalModule> = Vec::new();
    let config = CONFIG.load(deps.storage)?;
    let voting_module = VOTING_MODULE.load(deps.storage)?;
    let binding = &PROPOSAL_MODULES;
    let iter = binding.iter(deps.storage)?;
    for item in iter {
        let (_, proposal_module) = item?;
        proposal_module_res.push(proposal_module);
    }
    let pause_info = get_pause_info(deps, env)?;
    let version = get_contract_version(deps.storage)?;
    let active_proposal_module_count = ACTIVE_PROPOSAL_MODULE_COUNT.load(deps.storage)?;
    let total_proposal_module_count = TOTAL_PROPOSAL_MODULE_COUNT.load(deps.storage)?;
    to_binary(&DumpStateResponse {
        admin,
        config,
        version,
        pause_info,
        proposal_modules: proposal_module_res,
        voting_module,
        active_proposal_module_count,
        total_proposal_module_count,
    })
}

pub fn query_voting_power_at_height(
    deps: Deps,
    address: String,
    height: Option<u64>,
) -> StdResult<Binary> {
    let voting_module = VOTING_MODULE.load(deps.storage)?;
    let voting_power: voting::VotingPowerAtHeightResponse = deps.querier.query_wasm_smart(
        voting_module.code_hash,
        voting_module.addr,
        &voting::Query::VotingPowerAtHeight { height, address },
    )?;
    to_binary(&voting_power)
}

pub fn query_total_power_at_height(deps: Deps, height: Option<u64>) -> StdResult<Binary> {
    let voting_module = VOTING_MODULE.load(deps.storage)?;
    let total_power: voting::TotalPowerAtHeightResponse = deps.querier.query_wasm_smart(
        voting_module.code_hash,
        voting_module.addr,
        &voting::Query::TotalPowerAtHeight { height },
    )?;
    to_binary(&total_power)
}

pub fn query_get_item(deps: Deps, item: String) -> StdResult<Binary> {
    let item = ITEMS.get(deps.storage, &item);
    to_binary(&GetItemResponse { item })
}

pub fn query_info(deps: Deps) -> StdResult<Binary> {
    let info = secret_cw2::get_contract_version(deps.storage)?;
    to_binary(&dao_interface::voting::InfoResponse { info })
}

pub fn query_list_items(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Binary> {
    let mut res: Vec<(String, String)> = Vec::new(); // Vector to hold key-value pairs
    let mut start = start_after.clone();
    let binding = &ITEMS;
    let iter = binding.iter(deps.storage)?;
    
    for item in iter {
        let (key, value) = item?;
        if let Some(start_after) = &start {
            if &key == start_after {
                // If we found the start point, reset it to start iterating
                start = None;
            }
        }
        if start.is_none() {
            res.push((key.clone(), value.clone())); // Collect the key-value pair
            if res.len() >= limit.unwrap() as usize {
                break; // Break out of loop if limit reached
            }
        }
    }
    to_binary(&res)
}

pub fn query_cw20_list(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Binary> {
    // to_binary(&paginate_map_keys(
    //     deps,
    //     &SNIP20_LIST,
    //     0,
    //     SNIP20_LIST.get_len(deps.storage).unwrap_or_default(),
    // )?)

    let mut res:Vec<(String,String)>=Vec::new();
    let mut start=start_after.clone();
    let binding = &SNIP20_LIST;
    let iter = binding.iter(deps.storage)?;
    for item in iter {
        let ((addr,code_hash), _) = item?;
        if let Some(start_after) = &start {
            if &addr == start_after {
                // If we found the start point, reset it to start iterating
                start = None;
            }
        }
        if start.is_none() {
            res.push((addr.to_string(),code_hash));
            if res.len() >= limit.unwrap() as usize {
                break; // Break out of loop if limit reached
            }
        }
    }
    to_binary(&res)
}

pub fn query_cw721_list(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Binary> {
    // to_binary(&paginate_map_keys(
    //     deps,
    //     &SNIP721_LIST,
    //     0,
    //     SNIP721_LIST.get_len(deps.storage).unwrap_or_default(),
    // )?)

    let mut res:Vec<(String,String)>=Vec::new();
    let mut start=start_after.clone();
    let binding = &SNIP721_LIST;
    let iter = binding.iter(deps.storage)?;
    for item in iter {
        let ((addr,code_hash), _) = item?;
        if let Some(start_after) = &start {
            if &addr == start_after {
                // If we found the start point, reset it to start iterating
                start = None;
            }
        }
        if start.is_none() {
            res.push((addr.to_string(),code_hash));
            if res.len() >= limit.unwrap() as usize {
                break; // Break out of loop if limit reached
            }
        }
    }
    to_binary(&res)
}

pub fn query_cw20_balances(
    deps: Deps,
    env: Env,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Binary> {
   
    let mut res:Vec<(String,String)>=Vec::new();
    let mut start=start_after.clone();
    let binding = &SNIP20_LIST;
    let iter = binding.iter(deps.storage)?;
    for item in iter {
        let ((addr,code_hash), _) = item?;
        if let Some(start_after) = &start {
            if &addr == start_after {
                // If we found the start point, reset it to start iterating
                start = None;
            }
        }
        if start.is_none() {
            res.push((addr.to_string(),code_hash));
            if res.len() >= limit.unwrap() as usize {
                break; // Break out of loop if limit reached
            }
        }
    }
    let balances = res
        .into_iter()
        .map(|(addr, code_hash)| {
            let viewing_key = TOKEN_VIEWING_KEY.load(deps.storage).unwrap_or_default();
            let balance: snip20_reference_impl::msg::Balance = deps.querier.query_wasm_smart(
                code_hash.clone(),
                addr.clone(),
                &snip20_reference_impl::msg::QueryMsg::Balance {
                    address: env.contract.address.to_string(),
                    key: viewing_key,
                },
            )?;
            Ok(Snip20BalanceResponse {
                addr,
                balance: balance.amount,
            })
        })
        .collect::<StdResult<Vec<_>>>()?;
    to_binary(&balances)
}

pub fn query_list_sub_daos(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Binary> {
    let start_at = start_after.clone()
        .map(|addr| deps.api.addr_validate(&addr))
        .transpose()?;

        let mut subdaos:Vec<(Addr,Option<String>)>=Vec::new();
        let mut start=start_at.clone();
        let binding = &SUBDAO_LIST;
        let iter = binding.iter(deps.storage)?;
        for item in iter {
            let (addr,value ) = item?;
            if let Some(start_at) = &start {
                if &addr == start_at {
                    // If we found the start point, reset it to start iterating
                    start = None;
                }
            }
            if start.is_none() {
                subdaos.push((addr,value));
                if subdaos.len() >= limit.unwrap() as usize {
                    break; // Break out of loop if limit reached
                }
            }
        }

    let subdaos: Vec<SubDao> = subdaos
        .into_iter()
        .map(|(address, charter)| SubDao {
            addr: address.into_string(),
            charter,
        })
        .collect();

    to_binary(&subdaos)
}

pub fn query_dao_uri(deps: Deps) -> StdResult<Binary> {
    let config = CONFIG.load(deps.storage)?;
    to_binary(&DaoURIResponse {
        dao_uri: config.dao_uri,
    })
}

pub fn query_proposal_module_count(deps: Deps) -> StdResult<Binary> {
    to_binary(&ProposalModuleCountResponse {
        active_proposal_module_count: ACTIVE_PROPOSAL_MODULE_COUNT.load(deps.storage)?,
        total_proposal_module_count: TOTAL_PROPOSAL_MODULE_COUNT.load(deps.storage)?,
    })
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
        PROPOSAL_MODULE_REPLY_ID => {
            match msg.clone().result {
                cosmwasm_std::SubMsgResult::Ok(_) => {
                    let res = parse_reply_instantiate_data(msg)?;
                    let prop_module_addr = deps.api.addr_validate(&res.contract_address)?;
                    let total_module_count = TOTAL_PROPOSAL_MODULE_COUNT.load(deps.storage)?;

                    let prefix = derive_proposal_module_prefix(total_module_count as usize)?;
                    let prop_module = ProposalModule {
                        address: prop_module_addr.clone(),
                        status: ProposalModuleStatus::Enabled,
                        prefix,
                    };

                    PROPOSAL_MODULES.insert(deps.storage, &prop_module_addr, &prop_module)?;

                    // Save active and total proposal module counts.
                    ACTIVE_PROPOSAL_MODULE_COUNT
                        .update::<_, StdError>(deps.storage, |count| Ok(count + 1))?;
                    TOTAL_PROPOSAL_MODULE_COUNT.save(deps.storage, &(total_module_count + 1))?;

                    // Check for module instantiation callbacks
                    let callback_msgs = match res.data {
                        Some(data) => from_binary::<ModuleInstantiateCallback>(&data)
                            .map(|m| m.msgs)
                            .unwrap_or_else(|_| vec![]),
                        None => vec![],
                    };

                    Ok(Response::default()
                        .add_attribute("prop_module".to_string(), res.contract_address)
                        .add_messages(callback_msgs))
                }
                cosmwasm_std::SubMsgResult::Err(_) => Err(ContractError::InstantiateError {}),
            }
        }

        VOTE_MODULE_INSTANTIATE_REPLY_ID => {
            match msg.result {
                cosmwasm_std::SubMsgResult::Ok(_) => {
                    let res = parse_reply_instantiate_data(msg)?;
                    let vote_module_addr = deps.api.addr_validate(&res.contract_address)?;
                    let mut current = VOTING_MODULE.load(deps.storage)?;

                    // Make sure a bug in instantiation isn't causing us to
                    // make more than one voting module.
                    if current.addr != Addr::unchecked("") {
                        return Err(ContractError::MultipleVotingModules {});
                    }
                    current.addr = vote_module_addr.clone();

                    VOTING_MODULE.save(deps.storage, &current)?;

                    // Check for module instantiation callbacks
                    let callback_msgs = match res.data {
                        Some(data) => from_binary::<ModuleInstantiateCallback>(&data)
                            .map(|m| m.msgs)
                            .unwrap_or_else(|_| vec![]),
                        None => vec![],
                    };

                    Ok(Response::default()
                        .add_attribute("voting_module", vote_module_addr.clone())
                        .add_messages(callback_msgs))
                }
                cosmwasm_std::SubMsgResult::Err(_) => Err(ContractError::InstantiateError {}),
            }
        }

        VOTE_MODULE_UPDATE_REPLY_ID => match msg.result {
            cosmwasm_std::SubMsgResult::Ok(_) => {
                let res = parse_reply_instantiate_data(msg)?;
                let vote_module_addr = deps.api.addr_validate(&res.contract_address)?;
                let mut current = VOTING_MODULE.load(deps.storage)?;
                current.addr = vote_module_addr.clone();
                VOTING_MODULE.save(deps.storage, &current)?;

                Ok(Response::default().add_attribute("voting_module", vote_module_addr.clone()))
            }
            cosmwasm_std::SubMsgResult::Err(_) => Err(ContractError::InstantiateError {}),
        },
        SNIP20_VIEWING_KEY_REPLY_ID => {
            match msg.result {
                cosmwasm_std::SubMsgResult::Ok(res) => {
                    // let mut token_viewing_key=TOKEN_VIEWING_KEY.load(deps.storage).unwrap_or_default();
                    let data: snip20_reference_impl::msg::CreateViewingKeyResponse =
                        from_binary(&res.data.unwrap())?;
                    TOKEN_VIEWING_KEY.save(deps.storage, &data.key)?;
                    Ok(Response::new().add_attribute("action", "create_token_viewing_key"))
                }
                cosmwasm_std::SubMsgResult::Err(_) => Err(ContractError::TokenExecuteError {}),
            }
        }
        _ => Err(ContractError::UnknownReplyID {}),
    }
}

pub(crate) fn derive_proposal_module_prefix(mut dividend: usize) -> StdResult<String> {
    dividend += 1;
    // Pre-allocate string
    let mut prefix = String::with_capacity(10);
    loop {
        let remainder = (dividend - 1) % 26;
        dividend = (dividend - remainder) / 26;
        let remainder_str = std::str::from_utf8(&[(remainder + 65) as u8])?.to_owned();
        prefix.push_str(&remainder_str);
        if dividend == 0 {
            break;
        }
    }
    Ok(prefix.chars().rev().collect())
}

#[cfg(test)]
mod test {
    use crate::contract::derive_proposal_module_prefix;
    use std::collections::HashSet;

    #[test]
    fn test_prefix_generation() {
        assert_eq!("A", derive_proposal_module_prefix(0).unwrap());
        assert_eq!("B", derive_proposal_module_prefix(1).unwrap());
        assert_eq!("C", derive_proposal_module_prefix(2).unwrap());
        assert_eq!("AA", derive_proposal_module_prefix(26).unwrap());
        assert_eq!("AB", derive_proposal_module_prefix(27).unwrap());
        assert_eq!("BA", derive_proposal_module_prefix(26 * 2).unwrap());
        assert_eq!("BB", derive_proposal_module_prefix(26 * 2 + 1).unwrap());
        assert_eq!("CA", derive_proposal_module_prefix(26 * 3).unwrap());
        assert_eq!("JA", derive_proposal_module_prefix(26 * 10).unwrap());
        assert_eq!("YA", derive_proposal_module_prefix(26 * 25).unwrap());
        assert_eq!("ZA", derive_proposal_module_prefix(26 * 26).unwrap());
        assert_eq!("ZZ", derive_proposal_module_prefix(26 * 26 + 25).unwrap());
        assert_eq!("AAA", derive_proposal_module_prefix(26 * 26 + 26).unwrap());
        assert_eq!("YZA", derive_proposal_module_prefix(26 * 26 * 26).unwrap());
        assert_eq!("ZZ", derive_proposal_module_prefix(26 * 26 + 25).unwrap());
    }

    #[test]
    fn test_prefixes_no_collisions() {
        let mut seen = HashSet::<String>::new();
        for i in 0..25 * 25 * 25 {
            let prefix = derive_proposal_module_prefix(i).unwrap();
            if seen.contains(&prefix) {
                panic!("already seen value")
            }
            seen.insert(prefix);
        }
    }
}
