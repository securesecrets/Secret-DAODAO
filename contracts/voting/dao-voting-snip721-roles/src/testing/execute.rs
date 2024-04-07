use anyhow::Result as AnyResult;
use cosmwasm_std::{Addr, ContractInfo};
use secret_multi_test::{App, AppResponse, Executor};
use snip721_reference_impl::token::{Extension, Metadata};

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
        &snip721_reference_impl::msg::ExecuteMsg::MintNft {
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
                    role: Some("admin".to_string()),
                    weight: 1,
                }),
            }),
            private_metadata: None,
            serial_number: None,
            royalty_info: None,
            transferable: None,
            memo: None,
            padding: None,
        },
        &[],
    )
}
