#![doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))]

#[cfg(test)]
mod integration_tests;

use std::fmt::{self};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_binary, Addr, BankMsg, Coin, CosmosMsg, CustomQuery, Deps, QuerierWrapper, StdError,
    StdResult, Uint128, WasmMsg,
};

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum DenomError {
    #[error(transparent)]
    Std(#[from] StdError),

    #[error("invalid cw20 - did not respond to `TokenInfo` query: {err}")]
    InvalidCw20 { err: StdError },

    #[error("invalid native denom. length must be between in [3, 128], got ({len})")]
    NativeDenomLength { len: usize },

    #[error("expected alphabetic ascii character in native denomination")]
    NonAlphabeticAscii,

    #[error("invalid character ({c}) in native denom")]
    InvalidCharacter { c: char },
}

/// A denom that has been checked to point to a valid asset. This enum
/// should never be constructed literally and should always be built
/// by calling `into_checked` on an `UncheckedDenom` instance.
#[cw_serde]
pub enum CheckedDenom {
    /// A native (bank module) asset.
    Native(String),
    /// A cw20 asset.
    Cw20(Addr, String),
}

/// A denom that has not been checked to confirm it points to a valid
/// asset.
#[cw_serde]
pub enum UncheckedDenom {
    /// A native (bank module) asset.
    Native(String),
    /// A cw20 asset.
    Cw20(String, String),
}

impl UncheckedDenom {
    /// Converts an unchecked denomination into a checked one. In the
    /// case of native denominations, it is checked that the
    /// denomination is valid according to the [default SDK rules]. In
    /// the case of cw20 denominations the it is checked that the
    /// specified address is valid and that that address responds to a
    /// `TokenInfo` query without erroring and returns a valid
    /// `cw20::TokenInfoResponse`.
    ///
    /// [default SDK rules]: https://github.com/cosmos/cosmos-sdk/blob/7728516abfab950dc7a9120caad4870f1f962df5/types/coin.go#L865-L867
    pub fn into_checked(self, deps: Deps) -> Result<CheckedDenom, DenomError> {
        match self {
            Self::Native(denom) => validate_native_denom(denom),
            Self::Cw20(addr, code_hash) => {
                let addr = deps.api.addr_validate(&addr)?;
                let _info: secret_toolkit::snip20::TokenInfoResponse = deps
                    .querier
                    .query_wasm_smart(
                        code_hash.clone(),
                        addr.clone(),
                        &secret_toolkit::snip20::QueryMsg::TokenInfo {},
                    )
                    .map_err(|err| DenomError::InvalidCw20 { err })?;
                Ok(CheckedDenom::Cw20(addr, code_hash))
            }
        }
    }
}

impl CheckedDenom {
    /// Is the `CheckedDenom` this cw20?
    ///
    /// # Example
    ///
    /// ```
    /// use cosmwasm_std::{Addr, coin};
    /// use cw_denom::CheckedDenom;
    ///
    /// let cw20 = Addr::unchecked("fleesp");
    /// assert!(CheckedDenom::Cw20(Addr::unchecked("fleesp")).is_cw20(&cw20));
    /// assert!(!CheckedDenom::Native("fleesp".to_string()).is_cw20(&cw20));
    /// ```
    pub fn is_cw20(&self, cw20: &Addr) -> bool {
        match self {
            CheckedDenom::Native(_) => false,
            CheckedDenom::Cw20(a, _) => a == cw20,
        }
    }

    /// Is the `CheckedDenom` this native denom?
    ///
    /// # Example
    ///
    /// ```
    /// use cosmwasm_std::{Addr, coin};
    /// use cw_denom::CheckedDenom;
    ///
    /// let coin = coin(10, "floob");
    /// assert!(CheckedDenom::Native("floob".to_string()).is_native(&coin.denom));
    /// assert!(!CheckedDenom::Cw20(Addr::unchecked("floob")).is_native(&coin.denom));
    /// ```
    pub fn is_native(&self, denom: &str) -> bool {
        match self {
            CheckedDenom::Native(n) => n == denom,
            CheckedDenom::Cw20(..) => false,
        }
    }

    /// Queries WHO's balance for the denomination.
    pub fn query_balance<C: CustomQuery>(
        &self,
        querier: &QuerierWrapper<C>,
        who: &Addr,
        key: String,
    ) -> StdResult<Uint128> {
        match self {
            CheckedDenom::Native(denom) => Ok(querier.query_balance(who, denom)?.amount),
            CheckedDenom::Cw20(address, code_hash) => {
                let balance: secret_toolkit::snip20::Balance = querier.query_wasm_smart(
                    code_hash,
                    address,
                    &secret_toolkit::snip20::QueryMsg::Balance {
                        address: who.to_string(),
                        key,
                    },
                )?;
                Ok(balance.amount)
            }
        }
    }

    /// Gets a `CosmosMsg` that, when executed, will transfer AMOUNT
    /// tokens to WHO. AMOUNT being zero will cause the message
    /// execution to fail.
    pub fn get_transfer_to_message(&self, who: &Addr, amount: Uint128) -> StdResult<CosmosMsg> {
        Ok(match self {
            CheckedDenom::Native(denom) => BankMsg::Send {
                to_address: who.to_string(),
                amount: vec![Coin {
                    amount,
                    denom: denom.to_string(),
                }],
            }
            .into(),
            CheckedDenom::Cw20(address, code_hash) => WasmMsg::Execute {
                contract_addr: address.to_string(),
                code_hash: code_hash.to_string(),
                msg: to_binary(&secret_toolkit::snip20::HandleMsg::Transfer {
                    recipient: who.to_string(),
                    amount,
                    memo: None,
                    padding: None,
                })?,
                funds: vec![],
            }
            .into(),
        })
    }
}

/// Follows cosmos SDK validation logic. Specifically, the regex
/// string `[a-zA-Z][a-zA-Z0-9/:._-]{2,127}`.
///
/// <https://github.com/cosmos/cosmos-sdk/blob/7728516abfab950dc7a9120caad4870f1f962df5/types/coin.go#L865-L867>
pub fn validate_native_denom(denom: String) -> Result<CheckedDenom, DenomError> {
    if denom.len() < 3 || denom.len() > 128 {
        return Err(DenomError::NativeDenomLength { len: denom.len() });
    }
    let mut chars = denom.chars();
    // Really this means that a non utf-8 character is in here, but
    // non-ascii is also correct.
    let first = chars.next().ok_or(DenomError::NonAlphabeticAscii)?;
    if !first.is_ascii_alphabetic() {
        return Err(DenomError::NonAlphabeticAscii);
    }

    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '/' || c == ':' || c == '.' || c == '_' || c == '-')
        {
            return Err(DenomError::InvalidCharacter { c });
        }
    }

    Ok(CheckedDenom::Native(denom))
}

// Useful for returning these in response objects when updating the
// config or doing a withdrawal.
impl fmt::Display for CheckedDenom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Native(inner) => write!(f, "{inner}"),
            Self::Cw20(inner, _) => write!(f, "{inner}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env, MockQuerier},
        to_binary, Addr, ContractResult, QuerierResult, StdError, SystemError, Uint128, WasmQuery,
    };

    use super::*;

    const CW20_ADDR: &str = "cw20";
    const CW20_CODE_HASH: &str = "CODE_HASH";

    fn token_info_mock_querier(works: bool) -> impl Fn(&WasmQuery) -> QuerierResult {
        move |query: &WasmQuery| -> QuerierResult {
            match query {
                WasmQuery::Smart { contract_addr, .. } => {
                    if *contract_addr == CW20_ADDR {
                        if works {
                            QuerierResult::Ok(ContractResult::Ok(
                                to_binary(&secret_toolkit::snip20::TokenInfo {
                                    name: "coin".to_string(),
                                    symbol: "symbol".to_string(),
                                    decimals: 6,
                                    total_supply: Some(Uint128::new(10)),
                                })
                                .unwrap(),
                            ))
                        } else {
                            QuerierResult::Err(SystemError::NoSuchContract {
                                addr: CW20_ADDR.to_string(),
                            })
                        }
                    } else {
                        unimplemented!()
                    }
                }
                _ => unimplemented!(),
            }
        }
    }

    #[test]
    fn test_into_checked_cw20_valid() {
        let mut querier = MockQuerier::default();
        querier.update_wasm(token_info_mock_querier(true));

        let mut deps = mock_dependencies();
        deps.querier = querier;

        let unchecked = UncheckedDenom::Cw20(CW20_ADDR.to_string(), CW20_CODE_HASH.to_string());
        let checked = unchecked.into_checked(deps.as_ref()).unwrap();

        assert_eq!(
            checked,
            CheckedDenom::Cw20(Addr::unchecked(CW20_ADDR), CW20_CODE_HASH.to_string())
        )
    }

    #[test]
    fn test_into_checked_cw20_invalid() {
        let mut querier = MockQuerier::default();
        querier.update_wasm(token_info_mock_querier(false));

        let mut deps = mock_dependencies();
        deps.querier = querier;

        let unchecked = UncheckedDenom::Cw20(CW20_ADDR.to_string(), CW20_CODE_HASH.to_string());
        let err = unchecked.into_checked(deps.as_ref()).unwrap_err();
        assert_eq!(
            err,
            DenomError::InvalidCw20 {
                err: StdError::GenericErr {
                    msg: format!("Querier system error: No such contract: {CW20_ADDR}",)
                }
            }
        )
    }

    #[test]
    fn test_into_checked_cw20_addr_invalid() {
        let mut querier = MockQuerier::default();
        querier.update_wasm(token_info_mock_querier(true));

        let mut deps = mock_dependencies();
        deps.querier = querier;

        let unchecked = UncheckedDenom::Cw20(
            "HasCapitalsSoShouldNotValidate".to_string(),
            "HasCapitalsSoShouldNotValidate".to_string(),
        );
        let err = unchecked.into_checked(deps.as_ref()).unwrap_err();
        assert_eq!(
            err,
            DenomError::Std(StdError::GenericErr {
                msg: "Invalid input: address not normalized".to_string()
            })
        )
    }

    #[test]
    fn test_validate_native_denom_invalid() {
        let invalids = [
            // Too short.
            "ab".to_string(),
            // Too long.
            (0..129).map(|_| "a").collect::<String>(),
            // Starts with non alphabetic character.
            "1abc".to_string(),
            // Contains invalid character.
            "abc~d".to_string(),
            // Too short, also empty.
            "".to_string(),
            // Weird unicode start.
            "🥵abc".to_string(),
            // Weird unocide in non-head position.
            "ab:12🥵a".to_string(),
            // Comma is not a valid seperator.
            "ab,cd".to_string(),
        ];

        for invalid in invalids {
            assert!(validate_native_denom(invalid).is_err())
        }

        // Check that we're getting the errors we expect.
        assert_eq!(
            validate_native_denom("".to_string()),
            Err(DenomError::NativeDenomLength { len: 0 })
        );
        // Should check length before contents for better runtime.
        assert_eq!(
            validate_native_denom("1".to_string()),
            Err(DenomError::NativeDenomLength { len: 1 })
        );
        assert_eq!(
            validate_native_denom("🥵abc".to_string()),
            Err(DenomError::NonAlphabeticAscii)
        );
        // The regex that the SDK specifies works on ASCII characters
        // (not unicode classes), so this emoji has a "length" that is
        // greater than one (counted in terms of ASCII characters). As
        // such, we expect to fail on character validation and not
        // length.
        assert_eq!(
            validate_native_denom("🥵".to_string()),
            Err(DenomError::NonAlphabeticAscii)
        );
        assert_eq!(
            validate_native_denom("a🥵abc".to_string()),
            Err(DenomError::InvalidCharacter { c: '🥵' })
        );
    }

    #[test]
    fn test_validate_native_denom_valid() {
        let valids = [
            "ujuno",
            "uosmo",
            "IBC/A59A9C955F1AB8B76671B00C1A0482C64A6590352944BB5880E5122358F7E1CE",
            "wasm.juno123/channel-1/badkids",
        ];
        for valid in valids {
            validate_native_denom(valid.to_string()).unwrap();
        }
    }

    #[test]
    fn test_display() {
        let denom = CheckedDenom::Native("hello".to_string());
        assert_eq!(denom.to_string(), "hello".to_string());
        let denom = CheckedDenom::Cw20(Addr::unchecked("hello"), "CODE_HASH".to_string());
        assert_eq!(denom.to_string(), "hello".to_string());
    }
}
