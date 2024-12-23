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

/// Chainstate : Chainstate.
#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chainstate {
    /// Stacks block hash at the height.
    #[serde(rename = "stacksBlockHash")]
    pub stacks_block_hash: String,
    /// Stacks block height.
    #[serde(rename = "stacksBlockHeight")]
    pub stacks_block_height: u64,
}

impl Chainstate {
    /// Chainstate.
    pub fn new(stacks_block_hash: String, stacks_block_height: u64) -> Chainstate {
        Chainstate {
            stacks_block_hash,
            stacks_block_height,
        }
    }
}

