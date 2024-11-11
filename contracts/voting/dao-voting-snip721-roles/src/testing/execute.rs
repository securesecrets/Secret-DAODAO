use anyhow::Result as AnyResult;
use cosmwasm_std::{from_binary, Addr, ContractInfo};
use secret_multi_test::{App, AppResponse, Executor};
use snip721_roles::{ExecuteExt, MetadataExt};
use snip721_roles_impl::token::{Extension, Metadata};

pub fn mint_nft(
    app: &mut App,
    snip721_info: ContractInfo,
    sender: &str,
    receiver: Option<String>,
    token_id: Option<String>,
) -> AnyResult<AppResponse> {
    app.execute_contract(
        Addr::unchecked(sender),
        &snip721_info.clone(),
        &snip721_roles_impl::msg::ExecuteMsg::<snip721_roles::MetadataExt, ExecuteExt>::MintNft {
            token_id,
            owner: receiver,
            public_metadata: Some(Metadata {
                token_uri: None,
                extension: Some(Extension {
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
                }),
            }),
            private_metadata: None,
            serial_number: None,
            royalty_info: None,
            transferable: None,
            memo: None,
            padding: None,
            extension: MetadataExt {
                role: Some("admin".to_string()),
                weight: 1,
            },
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
        .execute_contract(Addr::unchecked(sender), &contract_info, &msg, &[])
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
