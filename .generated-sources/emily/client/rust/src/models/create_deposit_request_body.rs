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

/// CreateDepositRequestBody : Request structure for create deposit request.
#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreateDepositRequestBody {
    /// Output index on the bitcoin transaction associated with this specific deposit.
    #[serde(rename = "bitcoinTxOutputIndex")]
    pub bitcoin_tx_output_index: u32,
    /// Bitcoin transaction id.
    #[serde(rename = "bitcoinTxid")]
    pub bitcoin_txid: String,
    /// Deposit script.
    #[serde(rename = "depositScript")]
    pub deposit_script: String,
    /// Reclaim script.
    #[serde(rename = "reclaimScript")]
    pub reclaim_script: String,
}

impl CreateDepositRequestBody {
    /// Request structure for create deposit request.
    pub fn new(bitcoin_tx_output_index: u32, bitcoin_txid: String, deposit_script: String, reclaim_script: String) -> CreateDepositRequestBody {
        CreateDepositRequestBody {
            bitcoin_tx_output_index,
            bitcoin_txid,
            deposit_script,
            reclaim_script,
        }
    }
}

