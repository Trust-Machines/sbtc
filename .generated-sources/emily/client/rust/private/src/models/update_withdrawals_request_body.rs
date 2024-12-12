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

/// UpdateWithdrawalsRequestBody : Request structure for the create withdrawal request.
#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct UpdateWithdrawalsRequestBody {
    /// Withdrawal updates to execute.
    #[serde(rename = "withdrawals")]
    pub withdrawals: Vec<models::WithdrawalUpdate>,
}

impl UpdateWithdrawalsRequestBody {
    /// Request structure for the create withdrawal request.
    pub fn new(withdrawals: Vec<models::WithdrawalUpdate>) -> UpdateWithdrawalsRequestBody {
        UpdateWithdrawalsRequestBody {
            withdrawals,
        }
    }
}

