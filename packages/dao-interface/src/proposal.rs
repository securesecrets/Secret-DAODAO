use cosmwasm_schema::QueryResponses;
use schemars::JsonSchema;
use secret_cw2::ContractVersion;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub struct InfoResponse {
    pub info: ContractVersion,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
#[derive(QueryResponses)]
pub enum Query {
    /// Returns the address of the DAO this module belongs to
    #[returns(::cosmwasm_std::Addr)]
    Dao {},
    /// Returns contract version info
    #[returns(InfoResponse)]
    Info {},
    /// Returns the proposal ID that will be assigned to the
    /// next proposal created.
    #[returns(::std::primitive::u64)]
    NextProposalId {},
}

mod tests {
    /// Make sure the enum has all of the fields we expect. This will
    /// fail to compile if not.
    #[test]
    fn test_macro_expansion() {
        use super::Query;

        let query = Query::Info {};

        match query {
            Query::Dao {} => (),
            Query::Info {} => (),
            Query::NextProposalId {} => (),
        }
    }
}
