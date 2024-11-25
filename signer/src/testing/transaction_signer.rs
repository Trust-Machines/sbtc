//! Test utilities for the transaction signer

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::time::Duration;

use crate::blocklist_client;
use crate::context::Context;
use crate::context::SignerEvent;
use crate::context::SignerSignal;
use crate::context::TxSignerEvent;
use crate::ecdsa::SignEcdsa as _;
use crate::keys::PrivateKey;
use crate::keys::PublicKey;
use crate::message;
use crate::network;
use crate::network::in_memory2::WanNetwork;
use crate::network::MessageTransfer;
use crate::storage;
use crate::storage::model;
use crate::storage::DbRead;
use crate::storage::DbWrite;
use crate::testing;
use crate::testing::storage::model::TestData;
use crate::transaction_coordinator;
use crate::transaction_signer;

use rand::SeedableRng as _;
use sha2::Digest as _;
use tokio::sync::broadcast;
use tokio::time::error::Elapsed;
use wsts::net::SignatureType;

use super::context::*;

/// A test harness for the signer event loop.
pub struct TxSignerEventLoopHarness<Context, M, Rng> {
    context: Context,
    event_loop: EventLoop<Context, M, Rng>,
}

impl<Ctx, M, Rng> TxSignerEventLoopHarness<Ctx, M, Rng>
where
    Ctx: Context + 'static,
    Rng: rand::RngCore + rand::CryptoRng + Send + Sync + 'static,
    M: MessageTransfer + Send + Sync + 'static,
{
    /// Create the test harness.
    pub fn create(
        context: Ctx,
        network: M,
        context_window: u16,
        signer_private_key: PrivateKey,
        threshold: u32,
        rng: Rng,
    ) -> Self {
        Self {
            event_loop: transaction_signer::TxSignerEventLoop {
                context: context.clone(),
                network,
                signer_private_key,
                context_window,
                wsts_state_machines: HashMap::new(),
                threshold,
                rng,
                dkg_begin_pause: None,
            },
            context,
        }
    }

    /// Start the event loop.
    pub fn start(self) -> RunningEventLoopHandle<Ctx> {
        tokio::spawn(async { self.event_loop.run().await });

        RunningEventLoopHandle {
            signal_rx: self.context.get_signal_receiver(),
            context: self.context,
        }
    }
}

/// A running event loop.
pub struct RunningEventLoopHandle<C> {
    context: C,
    signal_rx: broadcast::Receiver<SignerSignal>,
}

impl<C> RunningEventLoopHandle<C>
where
    C: Context,
{
    /// Wait for `expected` instances of the given event `msg`, timing out after `timeout`.
    pub async fn wait_for_events(
        &mut self,
        msg: TxSignerEvent,
        expected: u16,
        timeout: Duration,
    ) -> Result<(), Elapsed> {
        let future = async {
            let mut n = 0;
            loop {
                if let Ok(SignerSignal::Event(SignerEvent::TxSigner(event))) =
                    self.signal_rx.recv().await
                {
                    if event == msg {
                        n += 1;
                    }

                    if n == expected {
                        return;
                    }
                }
            }
        };

        tokio::time::timeout(timeout, future).await
    }
}

type EventLoop<Context, M, Rng> = transaction_signer::TxSignerEventLoop<Context, M, Rng>;

impl blocklist_client::BlocklistChecker for () {
    async fn can_accept(
        &self,
        _address: &str,
    ) -> Result<bool, blocklist_api::apis::Error<blocklist_api::apis::address_api::CheckAddressError>>
    {
        Ok(true)
    }
}

/// Test environment.
pub struct TestEnvironment<C> {
    /// Function to construct a storage instance
    pub context: C,
    /// Bitcoin context window
    pub context_window: u16,
    /// Num signers
    pub num_signers: usize,
    /// Signing threshold
    pub signing_threshold: u32,
    /// Test model parameters
    pub test_model_parameters: testing::storage::model::Params,
}

impl<C> TestEnvironment<C>
where
    C: Context + 'static,
{
    /// Assert that the transaction signer will respond to bitcoin transaction sign requests
    /// with an acknowledge message. Errors after 10 seconds.
    pub async fn assert_should_respond_to_bitcoin_transaction_sign_requests(self) {
        let future = self.assert_should_respond_to_bitcoin_transaction_sign_requests_impl();
        tokio::time::timeout(Duration::from_secs(10), future)
            .await
            .unwrap()
    }

    /// Assert that the transaction signer will respond to bitcoin transaction sign requests
    /// with an acknowledge message
    pub async fn assert_should_respond_to_bitcoin_transaction_sign_requests_impl(self) {
        let mut rng = rand::rngs::StdRng::seed_from_u64(46);
        let wan_network = WanNetwork::default();
        let signer_info = testing::wsts::generate_signer_info(&mut rng, self.num_signers);
        let coordinator_signer_info = &signer_info.first().cloned().unwrap();

        let network = wan_network.connect(&self.context);

        let event_loop_harness = TxSignerEventLoopHarness::create(
            self.context.clone(),
            network.spawn(),
            self.context_window,
            coordinator_signer_info.signer_private_key,
            self.signing_threshold,
            rng.clone(),
        );

        let handle = event_loop_harness.start();

        let signer_private_key = signer_info.first().unwrap().signer_private_key.to_bytes();
        let dummy_aggregate_key = PublicKey::from_private_key(&PrivateKey::new(&mut rng));

        let signer_set = signer_info.first().unwrap().signer_public_keys.clone();
        store_dummy_dkg_shares(
            &mut rng,
            &signer_private_key,
            &handle.context.get_storage_mut(),
            dummy_aggregate_key,
            signer_set,
        )
        .await;

        let signer_set = &coordinator_signer_info.signer_public_keys;
        let test_data = self.generate_test_data(&mut rng, signer_set);
        Self::write_test_data(&handle.context.get_storage_mut(), &test_data).await;

        let bitcoin_chain_tip = handle
            .context
            .get_storage()
            .get_bitcoin_canonical_chain_tip()
            .await
            .expect("storage failure")
            .expect("no chain tip");

        let coordinator_public_key = transaction_coordinator::coordinator_public_key(
            &bitcoin_chain_tip,
            &signer_info.first().unwrap().signer_public_keys,
        )
        .unwrap();

        let coordinator_private_key = signer_info
            .iter()
            .find(|signer_info| {
                PublicKey::from_private_key(&signer_info.signer_private_key)
                    == coordinator_public_key
            })
            .unwrap()
            .signer_private_key;

        let transaction_sign_request = message::BitcoinTransactionSignRequest {
            tx: testing::dummy::tx(&fake::Faker, &mut rng),
            aggregate_key: dummy_aggregate_key,
        };

        run_dkg_and_store_results_for_signers(
            &signer_info,
            &bitcoin_chain_tip,
            self.signing_threshold,
            [handle.context.get_storage_mut()],
            &mut rng,
        )
        .await;

        let signer_instance = wan_network.connect(&self.context);
        let mut network_handle = signer_instance.spawn();

        let transaction_sign_request_payload: message::Payload = transaction_sign_request.into();

        network_handle
            .broadcast(
                transaction_sign_request_payload
                    .to_message(bitcoin_chain_tip)
                    .sign_ecdsa(&coordinator_private_key)
                    .expect("failed to sign"),
            )
            .await
            .expect("broadcast failed");

        let msg = network_handle
            .receive()
            .await
            .expect("failed to receive message");

        assert!(msg.verify());

        assert!(matches!(
            msg.payload,
            message::Payload::BitcoinTransactionSignAck(_)
        ));
    }

    /// Assert that a group of transaction signers together can
    /// participate successfully in a DKG round
    pub async fn assert_should_be_able_to_participate_in_dkg(self) {
        let mut rng = rand::rngs::StdRng::seed_from_u64(46);
        let network = network::InMemoryNetwork::new();
        let signer_info = testing::wsts::generate_signer_info(&mut rng, self.num_signers);
        let coordinator_signer_info = signer_info.first().unwrap().clone();

        // Create a new event-loop for each signer, based on the number of signers
        // defined in `self.num_signers`.
        let mut event_loop_handles: Vec<_> = signer_info
            .clone()
            .into_iter()
            .map(|signer_info| {
                let event_loop_harness = TxSignerEventLoopHarness::create(
                    TestContext::default_mocked(), // NEED TO HAVE A NEW CONTEXT FOR EACH SIGNER
                    network.connect(),
                    self.context_window,
                    signer_info.signer_private_key,
                    self.signing_threshold,
                    rng.clone(),
                );

                event_loop_harness.start()
            })
            .collect();

        let signer_set = &coordinator_signer_info.signer_public_keys;
        let test_data = self.generate_test_data(&mut rng, signer_set);
        for handle in event_loop_handles.iter_mut() {
            Self::write_test_data(&handle.context.get_storage_mut(), &test_data).await;
        }

        let bitcoin_chain_tip = event_loop_handles
            .first()
            .unwrap()
            .context
            .get_storage()
            .get_bitcoin_canonical_chain_tip()
            .await
            .expect("storage error")
            .expect("no chain tip");

        // now that we have a chain tip, get the real coordinator
        let coordinator_public_key =
            crate::transaction_coordinator::coordinator_public_key(&bitcoin_chain_tip, signer_set)
                .unwrap();
        let coordinator_signer_info = signer_info
            .iter()
            .find(|signer| {
                PublicKey::from_private_key(&signer.signer_private_key) == coordinator_public_key
            })
            .unwrap()
            .clone();

        run_dkg_and_store_results_for_signers(
            &signer_info,
            &bitcoin_chain_tip,
            self.signing_threshold,
            event_loop_handles
                .iter_mut()
                .map(|handle| handle.context.get_storage_mut()),
            &mut rng,
        )
        .await;

        let dummy_txid = testing::dummy::txid(&fake::Faker, &mut rng);

        let mut coordinator = testing::wsts::Coordinator::new(
            network.connect(),
            coordinator_signer_info,
            self.signing_threshold,
        );
        let aggregate_key = coordinator.run_dkg(bitcoin_chain_tip, dummy_txid).await;

        for handle in event_loop_handles.into_iter() {
            assert!(handle
                .context
                .get_storage()
                .get_encrypted_dkg_shares(&aggregate_key)
                .await
                .expect("storage error")
                .is_some());
        }
    }

    /// Assert that a group of transaction signers together can
    /// participate successfully in a signing roundd
    pub async fn assert_should_be_able_to_participate_in_signing_round(self) {
        let mut rng = rand::rngs::StdRng::seed_from_u64(46);
        let network = network::InMemoryNetwork::new();
        let signer_info = testing::wsts::generate_signer_info(&mut rng, self.num_signers);
        let coordinator_signer_info = signer_info.first().unwrap().clone();

        // A closure to build a new context for each signer
        let build_context = || {
            TestContext::builder()
                .with_in_memory_storage()
                .with_mocked_clients()
                .build()
        };

        let mut event_loop_handles: Vec<_> = signer_info
            .clone()
            .into_iter()
            .map(|signer_info| {
                let event_loop_harness = TxSignerEventLoopHarness::create(
                    build_context(),
                    network.connect(),
                    self.context_window,
                    signer_info.signer_private_key,
                    self.signing_threshold,
                    rng.clone(),
                );

                event_loop_harness.start()
            })
            .collect();

        let signer_set = &coordinator_signer_info.signer_public_keys;
        let test_data = self.generate_test_data(&mut rng, signer_set);
        for handle in event_loop_handles.iter_mut() {
            Self::write_test_data(&handle.context.get_storage_mut(), &test_data).await;
        }

        let bitcoin_chain_tip = event_loop_handles
            .first()
            .unwrap()
            .context
            .get_storage()
            .get_bitcoin_canonical_chain_tip()
            .await
            .expect("storage error")
            .expect("no chain tip");

        run_dkg_and_store_results_for_signers(
            &signer_info,
            &bitcoin_chain_tip,
            self.signing_threshold,
            event_loop_handles
                .iter_mut()
                .map(|handle| handle.context.get_storage_mut()),
            &mut rng,
        )
        .await;

        let coordinator_public_key = transaction_coordinator::coordinator_public_key(
            &bitcoin_chain_tip,
            &signer_info.first().unwrap().signer_public_keys,
        )
        .unwrap();

        let coordinator_signer_info = signer_info
            .iter()
            .find(|signer_info| {
                PublicKey::from_private_key(&signer_info.signer_private_key)
                    == coordinator_public_key
            })
            .unwrap()
            .clone();

        let dummy_txid = testing::dummy::txid(&fake::Faker, &mut rng);

        let mut coordinator = testing::wsts::Coordinator::new(
            network.connect(),
            coordinator_signer_info,
            self.signing_threshold,
        );

        let aggregate_key = coordinator.run_dkg(bitcoin_chain_tip, dummy_txid).await;

        let tx = testing::dummy::tx(&fake::Faker, &mut rng);
        let txid = tx.compute_txid();

        let mut hasher = sha2::Sha256::new();
        hasher.update("sign here please");
        let msg: [u8; 32] = hasher.finalize().into(); // TODO(296): Compute proper sighash from transaction

        coordinator
            .request_sign_transaction(bitcoin_chain_tip, tx, aggregate_key)
            .await;

        let signature = coordinator
            .run_signing_round(bitcoin_chain_tip, txid, &msg, SignatureType::Schnorr)
            .await;

        // Let's check the signature using the secp256k1 types.
        let sig = secp256k1::schnorr::Signature::from_slice(&signature.to_bytes()).unwrap();
        let msg_digest = secp256k1::Message::from_digest(msg);
        let x_only_pk = secp256k1::XOnlyPublicKey::from(&aggregate_key);
        sig.verify(&msg_digest, &x_only_pk).unwrap();

        // Let's check using the p256k1 types
        assert!(signature.verify(&p256k1::point::Point::from(aggregate_key).x(), &msg));
    }

    async fn write_test_data<S>(storage: &S, test_data: &TestData)
    where
        S: DbWrite,
    {
        test_data.write_to(storage).await;
    }

    fn generate_test_data<R>(&self, rng: &mut R, signer_set: &BTreeSet<PublicKey>) -> TestData
    where
        R: rand::RngCore,
    {
        let signer_keys: Vec<_> = signer_set.iter().copied().collect();
        TestData::generate(rng, &signer_keys, &self.test_model_parameters)
    }
}

async fn store_dummy_dkg_shares<R, S>(
    rng: &mut R,
    signer_private_key: &[u8; 32],
    storage: &S,
    group_key: PublicKey,
    signer_set: BTreeSet<PublicKey>,
) where
    R: rand::CryptoRng + rand::RngCore,
    S: storage::DbWrite,
{
    let mut shares =
        testing::dummy::encrypted_dkg_shares(&fake::Faker, rng, signer_private_key, group_key);
    shares.signer_set_public_keys = signer_set.into_iter().collect();

    storage
        .write_encrypted_dkg_shares(&shares)
        .await
        .expect("storage error");
}

/// This function runs a DKG round for the given signers and stores the
/// result in the provided stores for all signers.
async fn run_dkg_and_store_results_for_signers<'s: 'r, 'r, S, Rng>(
    signer_info: &[testing::wsts::SignerInfo],
    chain_tip: &model::BitcoinBlockHash,
    threshold: u32,
    stores: impl IntoIterator<Item = S>,
    rng: &mut Rng,
) where
    S: storage::DbRead + storage::DbWrite,
    Rng: rand::CryptoRng + rand::RngCore,
{
    let network = network::InMemoryNetwork::new();
    let mut testing_signer_set =
        testing::wsts::SignerSet::new(signer_info, threshold, || network.connect());
    let dkg_txid = testing::dummy::txid(&fake::Faker, rng);
    let bitcoin_chain_tip = *chain_tip;
    let (_, all_dkg_shares) = testing_signer_set
        .run_dkg(bitcoin_chain_tip, dkg_txid, rng)
        .await;

    for (storage, encrypted_dkg_shares) in stores.into_iter().zip(all_dkg_shares) {
        testing_signer_set
            .write_as_rotate_keys_tx(&storage, chain_tip, &encrypted_dkg_shares, rng)
            .await;

        storage
            .write_encrypted_dkg_shares(&encrypted_dkg_shares)
            .await
            .expect("failed to write encrypted shares");
    }
}
