use bitcoin::absolute::LockTime;
use bitcoin::hashes::Hash as _;
use bitcoin::transaction::Version;
use bitcoin::{AddressType, Amount, BlockHash, ScriptBuf, Sequence, Witness};
use bitcoincore_rpc::RpcApi as _;
use sbtc::testing::regtest::{p2wpkh_sign_transaction, AsUtxo as _, Recipient};
use signer::bitcoin::{BitcoinInteract, TransactionLookupHint};

use crate::docker;

#[cfg_attr(not(feature = "integration-tests-parallel"), ignore)]
#[tokio::test]
async fn test_get_block_not_found() {
    let bitcoind = docker::BitcoinCore::start().await;
    let client = bitcoind.get_client();
    let result = client.inner_client().get_block(&BlockHash::all_zeros());

    // This will return: JsonRpc(Rpc(RpcError { code: -5, message: "Block not found", data: None }))
    assert!(matches!(
        result.unwrap_err(),
        bitcoincore_rpc::Error::JsonRpc(bitcoincore_rpc::jsonrpc::Error::Rpc(rpc_error))
            if rpc_error.code == -5
    ))
}

// TODO: Figure out how to let this (and similar tests) run against the wallet
// generated by `initialize_blockchain()`. See comment in the test below.
//#[ignore = "This test needs to be run against a 'fresh' bitcoin core instance"]
#[cfg_attr(not(feature = "integration-tests-parallel"), ignore)]
#[tokio::test]
async fn test_get_block_works() {
    let bitcoind = docker::BitcoinCore::start().await;
    let client = bitcoind.get_client();
    let faucet = bitcoind.initialize_blockchain();

    let blocks = faucet.generate_blocks(5);

    // Double-check that an all-zero block doesn't return an error or something else unexpected.
    let block = client.get_block(&BlockHash::all_zeros());
    assert!(block.is_ok_and(|x| x.is_none()));

    for block in blocks.iter() {
        let b = client
            .get_block(block)
            .expect("failed to get block")
            .expect("expected to receive a block, not None");

        assert_eq!(b.header.block_hash(), *block);
    }
}

// TODO: Complete this test with inputs/outputs (it currently fails as the
// transaction is invalid). I didn't do this for now as it takes time, but I
// wanted to get a skeleton in place.
#[ignore = "This test needs to be completed (i.e. with inputs/outputs"]
#[tokio::test]
async fn broadcast_tx_works() {
    let bitcoind = docker::BitcoinCore::start().await;
    let client = bitcoind.get_client();

    let tx = bitcoin::Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![],
        output: vec![],
    };

    client.broadcast_transaction(&tx).await.unwrap();
}

#[cfg_attr(not(feature = "integration-tests-parallel"), ignore)]
#[tokio::test]
async fn calculate_transaction_fee_works_confirmed() {
    let bitcoind = docker::BitcoinCore::start().await;
    let client = bitcoind.get_client();
    let faucet = bitcoind.initialize_blockchain();
    let addr1 = Recipient::new(AddressType::P2wpkh);

    // Get some coins to spend (and our "utxo" outpoint).
    let outpoint = faucet.send_to(500_000, &addr1.address);
    // A coinbase transaction is not spendable until it has 100 confirmations.
    faucet.generate_blocks(1);

    // Get a utxo to spend (this method gives us an `AsUtxo` type which is
    // needed for signing below).
    let utxo = addr1
        .get_utxos(client.inner_client(), Some(1_000))
        .pop()
        .unwrap();
    assert_eq!(utxo.outpoint(), outpoint);

    // Create a transaction that spends the utxo.
    let mut tx = bitcoin::Transaction {
        version: Version::ONE,
        lock_time: LockTime::ZERO,
        input: vec![bitcoin::TxIn {
            previous_output: utxo.outpoint(),
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ZERO,
            witness: Witness::new(),
        }],
        output: vec![
            bitcoin::TxOut {
                value: Amount::from_sat(1_000),
                script_pubkey: addr1.address.script_pubkey(),
            },
            bitcoin::TxOut {
                value: utxo.amount - Amount::from_sat(1_000) * 2,
                script_pubkey: addr1.address.script_pubkey(),
            },
        ],
    };

    // Sign and broadcast the transaction
    p2wpkh_sign_transaction(&mut tx, 0, &utxo, &addr1.keypair);
    let txid = tx.compute_txid();
    client.broadcast_transaction(&tx).await.unwrap();
    // Confirm the transaction
    let block_hash = faucet.generate_blocks(1).pop().unwrap();

    let _ = client
        .get_tx_info(&txid, &block_hash)
        .unwrap()
        .expect("expected to be able to retrieve txinfo verbosity 2 for confirmed tx");

    let result = client
        .get_transaction_fee(&txid, Some(TransactionLookupHint::Confirmed))
        .await
        .expect("failed to calculate transaction fee");

    let expected_fee_total =
        utxo.amount.to_sat() - tx.output.iter().map(|o| o.value.to_sat()).sum::<u64>();
    let expected_fee_rate = expected_fee_total as f64 / tx.vsize() as f64;

    assert_eq!(result.fee, expected_fee_total);
    assert_eq!(result.fee_rate, expected_fee_rate);
}

#[cfg_attr(not(feature = "integration-tests-parallel"), ignore)]
#[tokio::test]
async fn calculate_transaction_fee_works_mempool() {
    let bitcoind = docker::BitcoinCore::start().await;
    let client = bitcoind.get_client();
    let faucet = bitcoind.initialize_blockchain();

    let addr1 = Recipient::new(AddressType::P2wpkh);

    // Get some coins to spend (and our "utxo" outpoint).
    let outpoint = faucet.send_to(500_000, &addr1.address);
    // A coinbase transaction is not spendable until it has 100 confirmations.
    faucet.generate_blocks(1);

    // Get a utxo to spend (this method gives us an `AsUtxo` type which is
    // needed for signing below).
    let utxo = addr1
        .get_utxos(client.inner_client(), Some(1_000))
        .pop()
        .unwrap();
    assert_eq!(utxo.outpoint(), outpoint);

    // Create a transaction that spends the utxo.
    let mut tx = bitcoin::Transaction {
        version: Version::ONE,
        lock_time: LockTime::ZERO,
        input: vec![bitcoin::TxIn {
            previous_output: utxo.outpoint(),
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ZERO,
            witness: Witness::new(),
        }],
        output: vec![
            bitcoin::TxOut {
                value: Amount::from_sat(1_000),
                script_pubkey: addr1.address.script_pubkey(),
            },
            bitcoin::TxOut {
                value: utxo.amount - Amount::from_sat(1_000) * 2,
                script_pubkey: addr1.address.script_pubkey(),
            },
        ],
    };

    // Sign and broadcast the transaction
    p2wpkh_sign_transaction(&mut tx, 0, &utxo, &addr1.keypair);
    client.broadcast_transaction(&tx).await.unwrap();

    let result = client
        .get_transaction_fee(&tx.compute_txid(), Some(TransactionLookupHint::Mempool))
        .await
        .expect("failed to calculate transaction fee");

    let expected_fee_total =
        utxo.amount.to_sat() - tx.output.iter().map(|o| o.value.to_sat()).sum::<u64>();
    let expected_fee_rate = expected_fee_total as f64 / tx.vsize() as f64;

    assert_eq!(result.fee, expected_fee_total);
    assert_eq!(result.fee_rate, expected_fee_rate);
}
