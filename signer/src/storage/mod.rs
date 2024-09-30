//! # Signer storage
//!
//! This module contains the `Read` and `Write` traits representing
//! the interface between the signer and their internal database.
//!
//! The canonical implementation of these traits is the [`postgres::PgStore`]
//! allowing the signer to use a Postgres database to store data.

pub mod in_memory;
pub mod model;
pub mod postgres;
pub mod sqlx;

use std::collections::HashSet;
use std::future::Future;

use blockstack_lib::types::chainstate::StacksBlockId;

use crate::bitcoin::utxo::SignerUtxo;
use crate::error::Error;
use crate::keys::PublicKey;
use crate::keys::SignerScriptPubKey as _;
use crate::stacks::events::CompletedDepositEvent;
use crate::stacks::events::WithdrawalAcceptEvent;
use crate::stacks::events::WithdrawalCreateEvent;
use crate::stacks::events::WithdrawalRejectEvent;

/// Represents the ability to read data from the signer storage.
pub trait DbRead {
    /// Get the bitcoin block with the given block hash.
    fn get_bitcoin_block(
        &self,
        block_hash: &model::BitcoinBlockHash,
    ) -> impl Future<Output = Result<Option<model::BitcoinBlock>, Error>> + Send;

    /// Get the stacks block with the given block hash.
    fn get_stacks_block(
        &self,
        block_hash: &model::StacksBlockHash,
    ) -> impl Future<Output = Result<Option<model::StacksBlock>, Error>> + Send;

    /// Get the bitcoin canonical chain tip.
    fn get_bitcoin_canonical_chain_tip(
        &self,
    ) -> impl Future<Output = Result<Option<model::BitcoinBlockHash>, Error>> + Send;

    /// Get the stacks chain tip, defined as the highest stacks block
    /// confirmed by the bitcoin chain tip.
    fn get_stacks_chain_tip(
        &self,
        bitcoin_chain_tip: &model::BitcoinBlockHash,
    ) -> impl Future<Output = Result<Option<model::StacksBlock>, Error>> + Send;

    /// Get pending deposit requests
    fn get_pending_deposit_requests(
        &self,
        chain_tip: &model::BitcoinBlockHash,
        context_window: u16,
    ) -> impl Future<Output = Result<Vec<model::DepositRequest>, Error>> + Send;

    /// Get pending deposit requests that have been accepted by at least
    /// `signatures_required` signers and has no responses
    fn get_pending_accepted_deposit_requests(
        &self,
        chain_tip: &model::BitcoinBlockHash,
        context_window: u16,
        signatures_required: u16,
    ) -> impl Future<Output = Result<Vec<model::DepositRequest>, Error>> + Send;

    /// Get the deposit requests that the signer has accepted to sign
    fn get_accepted_deposit_requests(
        &self,
        signer: &PublicKey,
    ) -> impl Future<Output = Result<Vec<model::DepositRequest>, Error>> + Send;

    /// Get signer decisions for a deposit request
    fn get_deposit_signers(
        &self,
        txid: &model::BitcoinTxId,
        output_index: u32,
    ) -> impl Future<Output = Result<Vec<model::DepositSigner>, Error>> + Send;

    /// Get signer decisions for a withdrawal request
    fn get_withdrawal_signers(
        &self,
        request_id: u64,
        block_hash: &model::StacksBlockHash,
    ) -> impl Future<Output = Result<Vec<model::WithdrawalSigner>, Error>> + Send;

    /// Get pending withdrawal requests
    fn get_pending_withdrawal_requests(
        &self,
        chain_tip: &model::BitcoinBlockHash,
        context_window: u16,
    ) -> impl Future<Output = Result<Vec<model::WithdrawalRequest>, Error>> + Send;

    /// Get pending withdrawal requests that have been accepted by at least
    /// `threshold` signers and has no responses
    fn get_pending_accepted_withdrawal_requests(
        &self,
        chain_tip: &model::BitcoinBlockHash,
        context_window: u16,
        threshold: u16,
    ) -> impl Future<Output = Result<Vec<model::WithdrawalRequest>, Error>> + Send;

    /// Get bitcoin blocks that include a particular transaction
    fn get_bitcoin_blocks_with_transaction(
        &self,
        txid: &model::BitcoinTxId,
    ) -> impl Future<Output = Result<Vec<model::BitcoinBlockHash>, Error>> + Send;

    /// Returns whether the given block ID is stored.
    fn stacks_block_exists(
        &self,
        block_id: StacksBlockId,
    ) -> impl Future<Output = Result<bool, Error>> + Send;

    /// Return the applicable DKG shares for the
    /// given aggregate key
    fn get_encrypted_dkg_shares(
        &self,
        aggregate_key: &PublicKey,
    ) -> impl Future<Output = Result<Option<model::EncryptedDkgShares>, Error>> + Send;

    /// Return the latest rotate-keys transaction confirmed by the given `chain-tip`.
    fn get_last_key_rotation(
        &self,
        chain_tip: &model::BitcoinBlockHash,
    ) -> impl Future<Output = Result<Option<model::RotateKeysTransaction>, Error>> + Send;

    /// Get the last 365 days worth of the signers' `scriptPubkey`s.
    fn get_signers_script_pubkeys(
        &self,
    ) -> impl Future<Output = Result<Vec<model::Bytes>, Error>> + Send;

    /// Get the outstanding signer UTXO.
    ///
    /// Under normal conditions, the signer will have only one UTXO they can spend.
    /// The specific UTXO we want is one such that:
    /// 1. The transaction is in a block on the canonical bitcoin blockchain.
    /// 2. The output is the first output in the transaction.
    /// 3. The output's `scriptPubKey` matches `aggregate_key`.
    /// 4. The output is unspent. It is possible for more than one transaction
    ///     within the same block to satisfy points 1-3, but if the signers
    ///     have one or more transactions within a block, exactly one output
    ///     satisfying points 1-3 will be unspent.
    /// 5. The block that includes the transaction that satisfies points 1-4 has the greatest height of all such blocks.
    fn get_signer_utxo(
        &self,
        chain_tip: &model::BitcoinBlockHash,
        aggregate_key: &crate::keys::PublicKey,
    ) -> impl Future<Output = Result<Option<SignerUtxo>, Error>> + Send;

    /// For the given outpoint and aggregate key, get the list all signer
    /// votes in the signer set.
    fn get_deposit_request_signer_votes(
        &self,
        txid: &model::BitcoinTxId,
        output_index: u32,
        aggregate_key: &PublicKey,
    ) -> impl Future<Output = Result<model::SignerVotes, Error>> + Send;

    /// For the given withdrawal request identifier, and aggregate key, get
    /// the list for how the signers voted against the request.
    fn get_withdrawal_request_signer_votes(
        &self,
        id: &model::QualifiedRequestId,
        aggregate_key: &PublicKey,
    ) -> impl Future<Output = Result<model::SignerVotes, Error>> + Send;

    /// Check that the given block hash is included in the canonical
    /// bitcoin blockchain, where the canonical blockchain is identified by
    /// the given `chain_tip`.
    fn in_canonical_bitcoin_blockchain(
        &self,
        chain_tip: &model::BitcoinBlockRef,
        block_ref: &model::BitcoinBlockRef,
    ) -> impl Future<Output = Result<bool, Error>> + Send;

    /// Fetch the bitcoin transaction that is included in the block
    /// identified by the block hash.
    fn get_bitcoin_tx(
        &self,
        txid: &model::BitcoinTxId,
        block_hash: &model::BitcoinBlockHash,
    ) -> impl Future<Output = Result<Option<model::BitcoinTx>, Error>> + Send;
}

/// Represents the ability to write data to the signer storage.
pub trait DbWrite {
    /// Write a bitcoin block.
    fn write_bitcoin_block(
        &self,
        block: &model::BitcoinBlock,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write a stacks block.
    fn write_stacks_block(
        &self,
        block: &model::StacksBlock,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write a deposit request.
    fn write_deposit_request(
        &self,
        deposit_request: &model::DepositRequest,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write many deposit requests.
    fn write_deposit_requests(
        &self,
        deposit_requests: Vec<model::DepositRequest>,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write a withdrawal request.
    fn write_withdrawal_request(
        &self,
        request: &model::WithdrawalRequest,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write a signer decision for a deposit request.
    fn write_deposit_signer_decision(
        &self,
        decision: &model::DepositSigner,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write a signer decision for a withdrawal request.
    fn write_withdrawal_signer_decision(
        &self,
        decision: &model::WithdrawalSigner,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write a raw transaction.
    fn write_transaction(
        &self,
        transaction: &model::Transaction,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write a connection between a bitcoin block and a transaction
    fn write_bitcoin_transaction(
        &self,
        bitcoin_transaction: &model::BitcoinTxRef,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write the bitcoin transactions to the data store.
    fn write_bitcoin_transactions(
        &self,
        txs: Vec<model::Transaction>,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write a connection between a stacks block and a transaction
    fn write_stacks_transaction(
        &self,
        stacks_transaction: &model::StacksTransaction,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write the stacks transactions to the data store.
    fn write_stacks_transactions(
        &self,
        txs: Vec<model::Transaction>,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write the stacks block ids and their parent block ids.
    fn write_stacks_block_headers(
        &self,
        headers: Vec<model::StacksBlock>,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write encrypted DKG shares
    fn write_encrypted_dkg_shares(
        &self,
        shares: &model::EncryptedDkgShares,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write rotate-keys transaction
    fn write_rotate_keys_transaction(
        &self,
        key_rotation: &model::RotateKeysTransaction,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write the withdrawal-reject event to the database.
    fn write_withdrawal_reject_event(
        &self,
        event: &WithdrawalRejectEvent,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write the withdrawal-accept event to the database.
    fn write_withdrawal_accept_event(
        &self,
        event: &WithdrawalAcceptEvent,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write the withdrawal-create event to the database.
    fn write_withdrawal_create_event(
        &self,
        event: &WithdrawalCreateEvent,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Write the completed deposit event to the database.
    fn write_completed_deposit_event(
        &self,
        event: &CompletedDepositEvent,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

pub(crate) fn get_utxo(
    aggregate_key: &PublicKey,
    sbtc_txs: Vec<bitcoin::Transaction>,
) -> Result<Option<SignerUtxo>, Error> {
    let script_pubkey = aggregate_key.signers_script_pubkey();

    let spent: HashSet<bitcoin::OutPoint> = sbtc_txs
        .iter()
        .flat_map(|tx| tx.input.iter().map(|txin| txin.previous_output))
        .collect();

    let utxos = sbtc_txs
        .iter()
        .flat_map(|tx| {
            if let Some(tx_out) = tx.output.first() {
                let outpoint = bitcoin::OutPoint::new(tx.compute_txid(), 0);
                if tx_out.script_pubkey == *script_pubkey && !spent.contains(&outpoint) {
                    return Some(SignerUtxo {
                        outpoint,
                        amount: tx_out.value.to_sat(),
                        // Txs are filtered based on the `aggregate_key` script pubkey
                        public_key: bitcoin::XOnlyPublicKey::from(aggregate_key),
                    });
                }
            }

            None
        })
        .collect::<Vec<_>>();

    match utxos[..] {
        [] => Ok(None),
        [utxo] => Ok(Some(utxo)),
        _ => Err(Error::TooManySignerUtxos),
    }
}
