use cosmwasm_std::{ContractInfo, StdResult};
use dao_interface::voting::{
    InfoResponse, TotalPowerAtHeightResponse, VotingPowerAtHeightResponse,
};
use secret_multi_test::App;
use shade_protocol::basic_staking::Auth;
use snip721_roles::QueryExt;
use snip721_roles_impl::msg::Minters;

use crate::{msg::QueryMsg, state::Config};

pub fn query_config(app: &App, module: ContractInfo) -> StdResult<Config> {
    let config = app.wrap().query_wasm_smart(
        module.code_hash,
        module.address.to_string(),
        &QueryMsg::Config {},
    )?;
    Ok(config)
}

pub fn query_voting_power(
    app: &App,
    module: ContractInfo,
    auth: Auth,
    height: Option<u64>,
) -> StdResult<VotingPowerAtHeightResponse> {
    let power = app.wrap().query_wasm_smart(
        module.code_hash,
        module.address.to_string(),
        &QueryMsg::VotingPowerAtHeight { auth, height },
    )?;
    Ok(power)
}

pub fn query_total_power(
    app: &App,
    module: ContractInfo,
    height: Option<u64>,
) -> StdResult<TotalPowerAtHeightResponse> {
    let power = app.wrap().query_wasm_smart(
        module.code_hash,
        module.address.to_string(),
        &QueryMsg::TotalPowerAtHeight { height },
    )?;
    Ok(power)
}

pub fn query_info(app: &App, module: ContractInfo) -> StdResult<InfoResponse> {
    let info = app.wrap().query_wasm_smart(
        module.code_hash,
        module.address.to_string(),
        &QueryMsg::Info {},
    )?;
    Ok(info)
}

pub fn query_minter(app: &App, nft: ContractInfo) -> StdResult<Minters> {
    let minters_res: Minters = app.wrap().query_wasm_smart(
        nft.code_hash,
        nft.address.to_string(),
        &snip721_roles_impl::msg::QueryMsg::<QueryExt>::Minters {},
    )?;
    Ok(minters_res)
}
