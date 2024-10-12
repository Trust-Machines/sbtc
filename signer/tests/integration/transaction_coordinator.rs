use std::collections::BTreeSet;
use std::collections::HashMap;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use fake::Fake as _;
use fake::Faker;
use futures::StreamExt;
use rand::SeedableRng as _;
use secp256k1::Keypair;
use signer::context::Context;
use signer::context::SignerEvent;
use signer::context::SignerSignal;
use signer::context::TxSignerEvent;
use signer::keys::PublicKey;
use signer::network;
use signer::storage::model;
use signer::storage::model::EncryptedDkgShares;
use signer::storage::model::RotateKeysTransaction;
use signer::storage::postgres::PgStore;
use signer::storage::DbRead as _;
use signer::storage::DbWrite as _;
use signer::testing;
use signer::testing::context::TestContext;
use signer::testing::context::*;
use signer::testing::storage::model::TestData;
use signer::transaction_coordinator::TxCoordinatorEventLoop;
use signer::transaction_signer::TxSignerEventLoop;

use crate::DATABASE_NUM;

/// The [`TxCoordinatorEventLoop::get_signer_set_and_aggregate_key`]
/// function is supposed to fetch the "current" signing set and the
/// aggregate key to use for bitcoin transactions. It attempts to get the
/// latest rotate-keys contract call transaction confirmed on the canonical
/// Stacks blockchain and falls back to the DKG shares table if no such
/// transaction can be found.
///
/// This tests that we prefer rotate keys transactions if it's available
/// but will use the DKG shares behavior is indeed the case.
#[cfg_attr(not(feature = "integration-tests"), ignore)]
#[tokio::test]
async fn get_signer_public_keys_and_aggregate_key_falls_back() {
    let db_num = DATABASE_NUM.fetch_add(1, Ordering::SeqCst);
    let db = testing::storage::new_test_database(db_num, true).await;

    let mut rng = rand::rngs::StdRng::seed_from_u64(51);

    let ctx = TestContext::builder()
        .with_storage(db.clone())
        .with_mocked_clients()
        .build();

    let network = network::in_memory::Network::new();

    let coord = TxCoordinatorEventLoop {
        network: network.connect(),
        context: ctx.clone(),
        context_window: 10000,
        private_key: ctx.config().signer.private_key,
        signing_round_max_duration: Duration::from_secs(10),
        threshold: 2,
        dkg_max_duration: Duration::from_secs(10),
    };

    // We need stacks blocks for the rotate-keys transactions.
    let test_params = testing::storage::model::Params {
        num_bitcoin_blocks: 10,
        num_stacks_blocks_per_bitcoin_block: 1,
        num_deposit_requests_per_block: 0,
        num_withdraw_requests_per_block: 0,
        num_signers_per_request: 0,
    };
    let test_data = TestData::generate(&mut rng, &[], &test_params);
    test_data.write_to(&db).await;

    // We always need the chain tip.
    let chain_tip = db.get_bitcoin_canonical_chain_tip().await.unwrap().unwrap();

    // We have no rows in the DKG shares table and no rotate-keys
    // transactions, so this should error
    let ans = coord.get_signer_set_and_aggregate_key(&chain_tip).await;
    assert!(ans.is_err());

    // Alright, lets write some DKG shares into the database. When we do
    // that the signer set should be considered whatever the signer set is
    // from our DKG shares.
    let shares: EncryptedDkgShares = Faker.fake_with_rng(&mut rng);
    db.write_encrypted_dkg_shares(&shares).await.unwrap();

    let (aggregate_key, signer_set) = coord
        .get_signer_set_and_aggregate_key(&chain_tip)
        .await
        .unwrap();

    let shares_signer_set: BTreeSet<PublicKey> =
        shares.signer_set_public_keys.iter().copied().collect();

    assert_eq!(shares.aggregate_key, aggregate_key);
    assert_eq!(shares_signer_set, signer_set);

    // Okay not we write a rotate-keys transaction into the database. To do
    // that we need the stacks chain tip, and a something in 3 different
    // tables...
    let stacks_chain_tip = db.get_stacks_chain_tip(&chain_tip).await.unwrap().unwrap();

    let rotate_keys: RotateKeysTransaction = Faker.fake_with_rng(&mut rng);
    let transaction = model::Transaction {
        txid: rotate_keys.txid.into_bytes(),
        tx: Vec::new(),
        tx_type: model::TransactionType::RotateKeys,
        block_hash: stacks_chain_tip.block_hash.into_bytes(),
    };
    let tx = model::StacksTransaction {
        txid: rotate_keys.txid,
        block_hash: stacks_chain_tip.block_hash,
    };

    db.write_transaction(&transaction).await.unwrap();
    db.write_stacks_transaction(&tx).await.unwrap();
    db.write_rotate_keys_transaction(&rotate_keys)
        .await
        .unwrap();

    // Alright, now that we have a rotate-keys transaction, we can check if
    // it is preferred over the DKG shares table.
    let (aggregate_key, signer_set) = coord
        .get_signer_set_and_aggregate_key(&chain_tip)
        .await
        .unwrap();

    let rotate_keys_signer_set: BTreeSet<PublicKey> =
        rotate_keys.signer_set.iter().copied().collect();

    assert_eq!(rotate_keys.aggregate_key, aggregate_key);
    assert_eq!(rotate_keys_signer_set, signer_set);

    testing::storage::drop_db(db).await;
}

/// Test that we run DKG if the coordinator notices that DKG has not been
/// run yet.
///
/// This test proceeds by doing the following:
/// 1. Create a database, an associated context, and a Keypair for each of the signers in the signing set.
/// 2. Populate each database with the same data. They now have the same view of the canonical bitcoin blockchain.
/// 3. Check that there are no DKG shares in the database.
/// 4. Start the transaction coordinator for the "first" signer. We could start it for all signers, but we only need it for one.
/// 5. Start the
///
/// Some of the preconditions for this test to run successfully includes
/// having bootstrap public keys that align with the [`Keypair`] returned
/// from the [`testing::wallet::regtest_bootstrap_wallet`] function.
#[cfg_attr(not(feature = "integration-tests"), ignore)]
#[tokio::test]
async fn run_dkg_from_scratch() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(51);
    let (_, signer_key_pairs): (_, [Keypair; 3]) = testing::wallet::regtest_bootstrap_wallet();

    let test_params = testing::storage::model::Params {
        num_bitcoin_blocks: 10,
        num_stacks_blocks_per_bitcoin_block: 1,
        num_deposit_requests_per_block: 0,
        num_withdraw_requests_per_block: 0,
        num_signers_per_request: 0,
    };
    let test_data = TestData::generate(&mut rng, &[], &test_params);

    let iter: Vec<(Keypair, TestData)> = signer_key_pairs
        .iter()
        .copied()
        .zip(std::iter::repeat_with(|| test_data.clone()))
        .collect();

    let signer_connections: Vec<(_, PgStore, Keypair)> = futures::stream::iter(iter)
        .then(|(kp, data)| async move {
            let db_num = DATABASE_NUM.fetch_add(1, Ordering::SeqCst);
            let db = testing::storage::new_test_database(db_num, true).await;
            let ctx = TestContext::builder()
                .with_storage(db.clone())
                .with_mocked_clients()
                .build();

            data.write_to(&db).await;

            (ctx, db, kp)
        })
        .collect::<Vec<_>>()
        .await;

    let network = network::in_memory::Network::new();

    let (ctx, db, keypair) = signer_connections.first().unwrap();

    let some_shares = db.get_last_encrypted_dkg_shares().await.unwrap();

    assert!(some_shares.is_none());

    let coord = TxCoordinatorEventLoop {
        network: network.connect(),
        context: ctx.clone(),
        context_window: 10000,
        private_key: keypair.secret_key().into(),
        signing_round_max_duration: Duration::from_secs(10),
        threshold: ctx.config().signer.bootstrap_signatures_required,
        dkg_max_duration: Duration::from_secs(10),
    };

    let start_count = Arc::new(AtomicU8::new(0));

    let tx_signer_processes = signer_connections
        .iter()
        .map(|(context, _, kp)| TxSignerEventLoop {
            network: network.connect(),
            threshold: context.config().signer.bootstrap_signatures_required as u32,
            context: context.clone(),
            context_window: 10000,
            blocklist_checker: Some(()),
            wsts_state_machines: HashMap::new(),
            signer_private_key: kp.secret_key().into(),
            rng: rand::rngs::OsRng,
        });

    let _fut = tx_signer_processes
        .map(|ev| {
            let counter = start_count.clone();
            tokio::spawn(async move {
                counter.fetch_add(1, Ordering::Relaxed);
                ev.run().await
            })
        })
        .collect::<Vec<_>>();
    let counter = start_count.clone();
    let _handle = tokio::spawn({
        counter.fetch_add(1, Ordering::Relaxed);
        coord.run()
    });

    while start_count.load(Ordering::SeqCst) < 4 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let event = SignerEvent::TxSigner(TxSignerEvent::NewRequestsHandled);
    ctx.get_signal_sender()
        .send(SignerSignal::Event(event))
        .unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;

    let some_shares = db.get_last_encrypted_dkg_shares().await.unwrap();

    assert!(some_shares.is_some());

    for (_, db, _) in signer_connections {
        testing::storage::drop_db(db).await;
    }
}
