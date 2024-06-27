use anyhow::Result as AnyResult;
use cosmwasm_std::{Addr, ContractInfo};
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
            extension: MetadataExt{
                role: Some("admin".to_string()),
                weight: 1,
            },
        },
        &[],
    )
}
