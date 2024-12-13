/*
 * emily-openapi-spec
 *
 * No description provided (generated by Openapi Generator https://github.com/openapitools/openapi-generator)
 *
 * The version of the OpenAPI document: 0.1.0
 *
 * Generated by: https://openapi-generator.tech
 */

use crate::models;
use serde::{Deserialize, Serialize};

/// AccountLimits : The representation of a limit for a specific account.
#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct AccountLimits {
    /// Represents the current sBTC limits.
    #[serde(
        rename = "pegCap",
        default,
        with = "::serde_with::rust::double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub peg_cap: Option<Option<u64>>,
    /// Per deposit cap. If none then the cap is the same as the global per deposit cap.
    #[serde(
        rename = "perDepositCap",
        default,
        with = "::serde_with::rust::double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub per_deposit_cap: Option<Option<u64>>,
    /// Per deposit minimum. If none then there is no minimum.
    #[serde(
        rename = "perDepositMinimum",
        default,
        with = "::serde_with::rust::double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub per_deposit_minimum: Option<Option<u64>>,
    /// Per withdrawal cap. If none then the cap is the same as the global per withdrawal cap.
    #[serde(
        rename = "perWithdrawalCap",
        default,
        with = "::serde_with::rust::double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub per_withdrawal_cap: Option<Option<u64>>,
    /// Per withdrawal minimum. If none then there is no minimum.
    #[serde(
        rename = "perWithdrawalMinimum",
        default,
        with = "::serde_with::rust::double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub per_withdrawal_minimum: Option<Option<u64>>,
}

impl AccountLimits {
    /// The representation of a limit for a specific account.
    pub fn new() -> AccountLimits {
        AccountLimits {
            peg_cap: None,
            per_deposit_cap: None,
            per_deposit_minimum: None,
            per_withdrawal_cap: None,
            per_withdrawal_minimum: None,
        }
    }
}
