use cosmwasm_schema::{cw_serde, QueryResponses};
use dao_dao_macros::{cw20_token_query, voting_module_query};
use secret_toolkit::utils::InitCallback;

#[cw_serde]
pub enum TokenInfo {
    Existing {
        address: String,
        code_hash: String,
    },
    New {
        code_id: u64,
        code_hash: String,
        label: String,
        name: String,
        symbol: String,
        decimals: u8,
        initial_balances: Vec<snip20_reference_impl::msg::InitialBalance>,
    },
}

#[cw_serde]
pub struct InstantiateMsg {
    pub token_info: TokenInfo,
    pub dao_code_hash: String,
}

impl InitCallback for InstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}

#[cw_serde]
pub enum ExecuteMsg {}

#[cw20_token_query]
#[voting_module_query]
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {}
