use cosmwasm_std::{from_binary, Addr, Binary, ContractInfo};
use secret_multi_test::{App, AppResponse, Executor};

use anyhow::Result as AnyResult;
use secret_utils::Duration;
use snip721_reference_impl::msg::ReceiverInfo;

use crate::msg::ExecuteMsg;

// Shorthand for an unchecked address.
macro_rules! addr {
    ($x:expr ) => {
        Addr::unchecked($x)
    };
}

pub fn send_nft(
    app: &mut App,
    snip721_contract_info: &ContractInfo,
    sender: &str,
    receiver_info: &ContractInfo,
    token_id: &str,
    msg: Binary,
) -> AnyResult<AppResponse> {
    app.execute_contract(
        addr!(sender),
        snip721_contract_info,
        &snip721_reference_impl::msg::ExecuteMsg::SendNft {
            contract: receiver_info.address.to_string(),
            receiver_info: Some(ReceiverInfo {
                recipient_code_hash: receiver_info.code_hash.clone(),
                also_implements_batch_receive_nft: Some(false),
            }),
            token_id: token_id.to_string(),
            msg: Some(msg),
            memo: None,
            padding: None,
        },
        &[],
    )
}

pub fn mint_nft(
    app: &mut App,
    snip721_contract_info: &ContractInfo,
    sender: &str,
    receiver: &str,
    token_id: &str,
) -> AnyResult<AppResponse> {
    app.execute_contract(
        addr!(sender),
        snip721_contract_info,
        &snip721_reference_impl::msg::ExecuteMsg::MintNft {
            token_id: Some(token_id.to_string()),
            owner: Some(receiver.to_string()),
            public_metadata: None,
            private_metadata: None,
            serial_number: None,
            royalty_info: None,
            transferable: Some(true),
            memo: None,
            padding: None,
        },
        &[],
    )
}

pub fn stake_nft(
    app: &mut App,
    snip721_contract_info: &ContractInfo,
    module: &ContractInfo,
    sender: &str,
    token_id: &str,
) -> AnyResult<AppResponse> {
    send_nft(
        app,
        snip721_contract_info,
        sender,
        module,
        token_id,
        Binary::default(),
    )
}

pub fn mint_and_stake_nft(
    app: &mut App,
    snip721_contract_info: &ContractInfo,
    module: &ContractInfo,
    sender: &str,
    token_id: &str,
) -> AnyResult<()> {
    mint_nft(app, snip721_contract_info, sender, sender, token_id)?;
    stake_nft(app, snip721_contract_info, module, sender, token_id)?;
    Ok(())
}

pub fn unstake_nfts(
    app: &mut App,
    module: &ContractInfo,
    sender: &str,
    token_ids: &[&str],
) -> AnyResult<AppResponse> {
    app.execute_contract(
        addr!(sender),
        module,
        &ExecuteMsg::Unstake {
            token_ids: token_ids.iter().map(|s| s.to_string()).collect(),
        },
        &[],
    )
}

pub fn update_config(
    app: &mut App,
    module: &ContractInfo,
    sender: &str,
    duration: Option<Duration>,
) -> AnyResult<AppResponse> {
    app.execute_contract(
        addr!(sender),
        module,
        &ExecuteMsg::UpdateConfig { duration },
        &[],
    )
}

pub fn claim_nfts(app: &mut App, module: &ContractInfo, sender: &str) -> AnyResult<AppResponse> {
    app.execute_contract(addr!(sender), module, &ExecuteMsg::ClaimNfts {}, &[])
}

pub fn add_hook(
    app: &mut App,
    module: &ContractInfo,
    sender: &str,
    hook: &str,
    hook_code_hash: String,
) -> AnyResult<AppResponse> {
    app.execute_contract(
        addr!(sender),
        module,
        &ExecuteMsg::AddHook {
            addr: hook.to_string(),
            code_hash: hook_code_hash,
        },
        &[],
    )
}

pub fn remove_hook(
    app: &mut App,
    module: &ContractInfo,
    sender: &str,
    hook: &str,
    hook_code_hash: String,
) -> AnyResult<AppResponse> {
    app.execute_contract(
        addr!(sender),
        module,
        &ExecuteMsg::RemoveHook {
            addr: hook.to_string(),
            code_hash: hook_code_hash,
        },
        &[],
    )
}

pub fn create_viewing_key(app: &mut App, contract_info: ContractInfo, sender: &str) -> String {
    let msg = shade_protocol::contract_interfaces::query_auth::ExecuteMsg::CreateViewingKey {
        entropy: "entropy".to_string(),
        padding: None,
    };
    let res = app
        .execute_contract(addr!(sender), &contract_info, &msg, &[])
        .unwrap();
    let mut viewing_key = String::new();
    let data: shade_protocol::contract_interfaces::query_auth::ExecuteAnswer =
        from_binary(&res.data.unwrap()).unwrap();
    if let shade_protocol::contract_interfaces::query_auth::ExecuteAnswer::CreateViewingKey {
        key,
    } = data
    {
        viewing_key = key;
    };
    viewing_key
}
