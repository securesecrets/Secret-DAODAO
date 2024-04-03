use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_binary, Addr, BankMsg, Coin, CosmosMsg, Deps, StdError, Uint128, WasmMsg};
use secret_storage_plus::Item;

use crate::{
    msg::{Counterparty, TokenInfo},
    ContractError,
};

#[cw_serde]
pub enum CheckedTokenInfo {
    Native {
        denom: String,
        amount: Uint128,
    },
    Snip20 {
        contract_addr: Addr,
        code_hash: String,
        amount: Uint128,
    },
}

#[cw_serde]
pub struct CheckedCounterparty {
    pub address: Addr,
    pub promise: CheckedTokenInfo,
    pub provided: bool,
}

pub const COUNTERPARTY_ONE: Item<CheckedCounterparty> = Item::new("counterparty_one");
pub const COUNTERPARTY_TWO: Item<CheckedCounterparty> = Item::new("counterparty_two");

impl Counterparty {
    pub fn into_checked(self, deps: Deps) -> Result<CheckedCounterparty, ContractError> {
        Ok(CheckedCounterparty {
            address: deps.api.addr_validate(&self.address)?,
            provided: false,
            promise: self.promise.into_checked(deps)?,
        })
    }
}

impl TokenInfo {
    pub fn into_checked(self, deps: Deps) -> Result<CheckedTokenInfo, ContractError> {
        match self {
            TokenInfo::Native { denom, amount } => {
                if amount.is_zero() {
                    Err(ContractError::ZeroTokens {})
                } else {
                    Ok(CheckedTokenInfo::Native { denom, amount })
                }
            }
            TokenInfo::Snip20 {
                contract_addr,
                code_hash,
                amount,
            } => {
                if amount.is_zero() {
                    Err(ContractError::ZeroTokens {})
                } else {
                    let contract_addr = deps.api.addr_validate(&contract_addr)?;
                    // Make sure we are dealing with a cw20.
                    let _: snip20_reference_impl::msg::QueryAnswer =
                        deps.querier.query_wasm_smart(
                            code_hash.clone(),
                            contract_addr.clone(),
                            &snip20_reference_impl::msg::QueryMsg::TokenInfo {},
                        )?;
                    Ok(CheckedTokenInfo::Snip20 {
                        contract_addr,
                        code_hash: code_hash.clone(),
                        amount,
                    })
                }
            }
        }
    }
}

impl CheckedTokenInfo {
    pub fn into_send_message(self, recipient: &Addr) -> Result<CosmosMsg, StdError> {
        Ok(match self {
            Self::Native { denom, amount } => BankMsg::Send {
                to_address: recipient.to_string(),
                amount: vec![Coin { denom, amount }],
            }
            .into(),
            Self::Snip20 {
                contract_addr,
                code_hash,
                amount,
            } => WasmMsg::Execute {
                contract_addr: contract_addr.into_string(),
                code_hash,
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Transfer {
                    recipient: recipient.to_string(),
                    amount,
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })?,
                funds: vec![],
            }
            .into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_into_spend_message_native() {
        let info = CheckedTokenInfo::Native {
            amount: Uint128::new(100),
            denom: "uekez".to_string(),
        };
        let message = info.into_send_message(&Addr::unchecked("ekez")).unwrap();

        assert_eq!(
            message,
            CosmosMsg::Bank(BankMsg::Send {
                to_address: "ekez".to_string(),
                amount: vec![Coin {
                    amount: Uint128::new(100),
                    denom: "uekez".to_string()
                }]
            })
        );
    }

    #[test]
    fn test_into_spend_message_cw20() {
        let info = CheckedTokenInfo::Snip20 {
            amount: Uint128::new(100),
            contract_addr: Addr::unchecked("ekez_token"),
            code_hash: "ekez_token_code_hash".to_string(),
        };
        let message = info.into_send_message(&Addr::unchecked("ekez")).unwrap();

        assert_eq!(
            message,
            CosmosMsg::Wasm(WasmMsg::Execute {
                funds: vec![],
                contract_addr: "ekez_token".to_string(),
                code_hash: "ekez_token_code_hash".to_string(),
                msg: to_binary(&snip20_reference_impl::msg::ExecuteMsg::Transfer {
                    recipient: "ekez".to_string(),
                    amount: Uint128::new(100),
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })
                .unwrap()
            })
        );
    }
}
