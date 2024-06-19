use cosmwasm_std::{ContractInfo, StdResult, Uint128};
use dao_interface::voting::{
    InfoResponse, TotalPowerAtHeightResponse, VotingPowerAtHeightResponse,
};
use secret_cw_controllers::HooksResponse;
use secret_multi_test::App;
use shade_protocol::basic_staking::Auth;
use snip721_controllers::NftClaimsResponse;

use crate::{msg::QueryMsg, state::Config};

pub fn query_config(app: &App, module: &ContractInfo) -> StdResult<Config> {
    let config = app.wrap().query_wasm_smart(
        module.code_hash.clone(),
        module.address.to_string(),
        &QueryMsg::Config {},
    )?;
    Ok(config)
}

pub fn query_claims(app: &App, module: &ContractInfo, auth: Auth) -> StdResult<NftClaimsResponse> {
    let claims = app.wrap().query_wasm_smart(
        module.code_hash.clone(),
        module.address.to_string(),
        &QueryMsg::NftClaims { auth },
    )?;
    Ok(claims)
}

pub fn query_hooks(app: &App, module: &ContractInfo) -> StdResult<HooksResponse> {
    let hooks = app.wrap().query_wasm_smart(
        module.code_hash.clone(),
        module.address.to_string(),
        &QueryMsg::Hooks {},
    )?;
    Ok(hooks)
}

pub fn query_staked_nfts(app: &App, module: &ContractInfo, auth: Auth) -> StdResult<Uint128> {
    let nfts = app.wrap().query_wasm_smart(
        module.code_hash.clone(),
        module.address.to_string(),
        &QueryMsg::StakedNfts { auth },
    )?;
    Ok(nfts)
}

pub fn query_voting_power(
    app: &App,
    module: &ContractInfo,
    auth: Auth,
    height: Option<u64>,
) -> StdResult<VotingPowerAtHeightResponse> {
    let power = app.wrap().query_wasm_smart(
        module.code_hash.clone(),
        module.address.to_string(),
        &QueryMsg::VotingPowerAtHeight { auth, height },
    )?;
    Ok(power)
}

pub fn query_total_power(
    app: &App,
    module: &ContractInfo,
    height: Option<u64>,
) -> StdResult<TotalPowerAtHeightResponse> {
    let power = app.wrap().query_wasm_smart(
        module.code_hash.clone(),
        module.address.to_string(),
        &QueryMsg::TotalPowerAtHeight { height },
    )?;
    Ok(power)
}

pub fn query_info(app: &App, module: &ContractInfo) -> StdResult<InfoResponse> {
    let info = app.wrap().query_wasm_smart(
        module.code_hash.clone(),
        module.address.to_string(),
        &QueryMsg::Info {},
    )?;
    Ok(info)
}

pub fn query_total_and_voting_power(
    app: &App,
    module: &ContractInfo,
    auth: Auth,
    height: Option<u64>,
) -> StdResult<(Uint128, Uint128)> {
    let total_power = query_total_power(app, module, height)?;
    let voting_power = query_voting_power(app, module, auth, height)?;

    Ok((total_power.power, voting_power.power))
}

pub fn query_nft_owner(
    app: &App,
    nft: &ContractInfo,
    token_id: &str,
) -> StdResult<snip721_reference_impl::msg::OwnerOf> {
    let owner = app.wrap().query_wasm_smart(
        nft.code_hash.clone(),
        nft.address.to_string(),
        &snip721_reference_impl::msg::QueryMsg::OwnerOf {
            token_id: token_id.to_string(),
            viewer: None,
            include_expired: None,
        },
    )?;
    Ok(owner)
}
