use cosmwasm_schema::cw_serde;
use cosmwasm_std::Binary;
use secret_toolkit::utils::InitCallback;
use shade_protocol::Contract;

#[cw_serde]
pub struct QueryAuthInstantiateMsg {
    pub admin_auth: Contract,
    pub prng_seed: Binary,
}

impl InitCallback for QueryAuthInstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}
