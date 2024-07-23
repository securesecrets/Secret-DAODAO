use cosmwasm_std::StdError;
use shade_protocol::{
    c_std::{Addr, Deps, StdResult},
    contract_interfaces::query_auth::{
        auth::{Key, PermitKey},
        Admin, ContractStatus, QueryAnswer, QueryPermit,
    },
    utils::storage::plus::{ItemStorage, MapStorage},
};

pub fn config(deps: Deps) -> StdResult<QueryAnswer> {
    Ok(QueryAnswer::Config {
        admin: Admin::load(deps.storage)?.0,
        state: ContractStatus::load(deps.storage)?,
    })
}

pub fn validate_vk(deps: Deps, user: Addr, key: String) -> StdResult<QueryAnswer> {
    Ok(QueryAnswer::ValidateViewingKey {
        is_valid: Key::verify(deps.storage, user, key)?,
    })
}

pub fn validate_permit(deps: Deps, permit: QueryPermit) -> StdResult<QueryAnswer> {
    if permit.params.key != Admin::load(deps.storage)?.0.address {
        return Err(StdError::generic_err("Invalid permit Key"));
    }
    let user = permit.validate(deps.api, None)?.as_addr(None)?;

    Ok(QueryAnswer::ValidatePermit {
        user: user.clone(),
        is_revoked: PermitKey::may_load(deps.storage, (user, permit.params.key))?.is_some(),
    })
}
