//! Utilities for constructing and loading WSTS state machines

use std::collections::BTreeMap;

use crate::codec::Decode as _;
use crate::codec::Encode as _;
use crate::error;
use crate::error::Error;
use crate::keys::SignerScriptPubkey;
use crate::storage;
use crate::storage::model;

use wsts::state_machine::coordinator::Coordinator as _;
use wsts::state_machine::coordinator::State as WstsState;
use wsts::state_machine::StateMachine as _;
use wsts::traits::Signer as _;

/// Wrapper around a WSTS signer state machine
#[derive(Debug, Clone, PartialEq)]
pub struct SignerStateMachine(wsts::state_machine::signer::Signer<wsts::v2::Party>);

type WstsStateMachine = wsts::state_machine::signer::Signer<wsts::v2::Party>;

impl SignerStateMachine {
    /// Create a new state machine
    pub fn new(
        signers: impl IntoIterator<Item = p256k1::ecdsa::PublicKey>,
        threshold: u32,
        signer_private_key: p256k1::scalar::Scalar,
    ) -> Result<Self, error::Error> {
        let signer_pub_key = p256k1::ecdsa::PublicKey::new(&signer_private_key)?;
        let signers: hashbrown::HashMap<u32, _> = signers
            .into_iter()
            .enumerate()
            .map(|(id, key)| {
                id.try_into()
                    .map(|id| (id, key))
                    .map_err(|_| error::Error::TypeConversion)
            })
            .collect::<Result<_, _>>()?;

        let key_ids = signers
            .clone()
            .into_iter()
            .map(|(id, key)| (id + 1, key))
            .collect();

        let num_parties = signers
            .len()
            .try_into()
            .map_err(|_| error::Error::TypeConversion)?;
        let num_keys = num_parties;

        let id: u32 = *signers
            .iter()
            .find(|(_, key)| *key == &signer_pub_key)
            .ok_or_else(|| error::Error::MissingPublicKey)?
            .0;

        let public_keys = wsts::state_machine::PublicKeys { signers, key_ids };

        let key_ids = vec![id + 1];

        if threshold > num_keys {
            return Err(error::Error::InvalidConfiguration);
        };

        let state_machine = WstsStateMachine::new(
            threshold,
            num_parties,
            num_keys,
            id,
            key_ids,
            signer_private_key,
            public_keys,
        );

        Ok(Self(state_machine))
    }

    /// Create a state machine from loaded DKG shares for the given aggregate key
    pub async fn load<S>(
        storage: &mut S,
        aggregate_key: p256k1::point::Point,
        signers: impl IntoIterator<Item = p256k1::ecdsa::PublicKey>,
        threshold: u32,
        signer_private_key: p256k1::scalar::Scalar,
    ) -> Result<Self, error::Error>
    where
        S: storage::DbRead + storage::DbWrite,
        error::Error: From<<S as storage::DbRead>::Error>,
        error::Error: From<<S as storage::DbWrite>::Error>,
    {
        let encrypted_shares = storage
            .get_encrypted_dkg_shares(&aggregate_key.x().to_bytes().to_vec())
            .await?
            .ok_or(error::Error::MissingDkgShares)?;

        let decrypted = wsts::util::decrypt(
            &signer_private_key.to_bytes(),
            &encrypted_shares.encrypted_private_shares,
        )
        .map_err(|_| error::Error::Encryption)?;

        let saved_state =
            wsts::traits::SignerState::decode(decrypted.as_slice()).map_err(error::Error::Codec)?;

        // This may panic if the saved state doesn't contain exactly one party,
        // however, that should never be the case since wsts maintains this invariant
        // when we save the state.
        let signer = wsts::v2::Party::load(&saved_state);

        let mut state_machine = Self::new(signers, threshold, signer_private_key)?;

        state_machine.0.signer = signer;

        Ok(state_machine)
    }

    /// Get the encrypted DKG shares
    pub fn get_encrypted_dkg_shares<Rng: rand::CryptoRng + rand::RngCore>(
        &self,
        rng: &mut Rng,
    ) -> Result<model::EncryptedDkgShares, error::Error> {
        let saved_state = self.signer.save();
        let aggregate_key = saved_state.group_key.x().to_bytes().to_vec();
        let tweaked_aggregate_key = wsts::compute::tweaked_public_key(&saved_state.group_key, None);

        let encoded = saved_state.encode_to_vec().map_err(error::Error::Codec)?;
        let public_shares = self
            .dkg_public_shares
            .encode_to_vec()
            .map_err(error::Error::Codec)?;

        let encrypted_private_shares =
            wsts::util::encrypt(&self.0.network_private_key.to_bytes(), &encoded, rng)
                .map_err(|_| error::Error::Encryption)?;

        let created_at = time::OffsetDateTime::now_utc();

        Ok(model::EncryptedDkgShares {
            aggregate_key: aggregate_key.clone(),
            tweaked_aggregate_key: tweaked_aggregate_key.x().to_bytes().to_vec(),
            script_pubkey: tweaked_aggregate_key.signers_script_pubkey().to_bytes(),
            encrypted_private_shares,
            public_shares,
            created_at,
        })
    }
}

impl std::ops::Deref for SignerStateMachine {
    type Target = WstsStateMachine;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for SignerStateMachine {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Wrapper around a WSTS coordinator state machine
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinatorStateMachine(WstsCoordinator);

type WstsCoordinator = wsts::state_machine::coordinator::frost::Coordinator<wsts::v2::Aggregator>;

impl CoordinatorStateMachine {
    /// Create a new state machine
    pub fn new<I>(signers: I, threshold: u32, message_private_key: p256k1::scalar::Scalar) -> Self
    where
        I: IntoIterator<Item = p256k1::ecdsa::PublicKey>,
    {
        let signer_public_keys: hashbrown::HashMap<u32, _> = signers
            .into_iter()
            .enumerate()
            .map(|(idx, key)| {
                (
                    idx.try_into().unwrap(),
                    (&p256k1::point::Compressed::from(key.to_bytes()))
                        .try_into()
                        .expect("failed to convert public key"),
                )
            })
            .collect();

        // The number of possible signers is capped at a number well below
        // u32::MAX, so this conversion should always work.
        let num_signers: u32 = signer_public_keys
            .len()
            .try_into()
            .expect("The number of signers is creater than u32::MAX?");
        let signer_key_ids = (0..num_signers)
            .map(|signer_id| (signer_id, std::iter::once(signer_id).collect()))
            .collect();
        let config = wsts::state_machine::coordinator::Config {
            num_signers,
            num_keys: num_signers,
            threshold,
            dkg_threshold: num_signers,
            message_private_key,
            dkg_public_timeout: None,
            dkg_private_timeout: None,
            dkg_end_timeout: None,
            nonce_timeout: None,
            sign_timeout: None,
            signer_key_ids,
            signer_public_keys,
        };

        let wsts_coordinator = WstsCoordinator::new(config);
        Self(wsts_coordinator)
    }

    /// Create a new coordinator state machine from the given aggregate
    /// key.
    ///
    /// # Notes
    ///
    /// The `WstsCoordinator` is a state machine that is responsible for
    /// DKG and for facilitating signing rounds. When created the
    /// `WstsCoordinator` state machine starts off in the `IDLE` state,
    /// where you can either start a signing round or start DKG. This
    /// function is for loading the state with the assumption that DKG has
    /// already been successfully completed.
    pub async fn load<I, S>(
        storage: &mut S,
        aggregate_key: p256k1::point::Point,
        signers: I,
        threshold: u32,
        message_private_key: p256k1::scalar::Scalar,
    ) -> Result<Self, Error>
    where
        I: IntoIterator<Item = p256k1::ecdsa::PublicKey>,
        S: storage::DbRead + storage::DbWrite,
        Error: From<<S as storage::DbRead>::Error>,
        Error: From<<S as storage::DbWrite>::Error>,
    {
        let encrypted_shares = storage
            .get_encrypted_dkg_shares(&aggregate_key.x().to_bytes().to_vec())
            .await?
            .ok_or(Error::MissingDkgShares)?;

        let public_dkg_shares: BTreeMap<u32, wsts::net::DkgPublicShares> =
            BTreeMap::decode(encrypted_shares.public_shares.as_slice()).map_err(Error::Codec)?;

        let mut coordinator = Self::new(signers, threshold, message_private_key);

        // The `coordinator` is a state machine that starts off in the
        // `IDLE` state, but we need to move it into a state where it can
        // accept the above public DKG shares. To do that we need to move
        // it to the `DKG_PUBLIC_GATHER` state and make sure that it is
        // properly initialized. The way to do that is to process a
        // `DKG_BEGIN` message, it will automatically move the state of the
        // machine to the `DKG_PUBLIC_GATHER` state.
        let packet = wsts::net::Packet {
            msg: wsts::net::Message::DkgBegin(wsts::net::DkgBegin { dkg_id: 1 }),
            sig: Vec::new(),
        };
        // If WSTS thinks that the we've already completed DKG for the
        // given ID, then it will return with `(None, None)`. This only
        // happens when the coordinator's `dkg_id` is greater than or equal
        // to the value given in the message. But the coordinator's dkg_id
        // starts at 0 and we start our's at 1.
        let (Some(_), _) = coordinator
            .process_message(&packet)
            .map_err(coordinator_error)?
        else {
            let msg = "Bad DKG id given".to_string();
            let err = wsts::state_machine::coordinator::Error::BadStateChange(msg);
            return Err(coordinator_error(err));
        };

        // TODO(338): Replace this for-loop with a simpler method to set
        // the public DKG shares.
        //
        // In this part we are trying to set the party_polynomials of the
        // WstsCoordinator given all of the known public keys that we
        // stored in the database.
        for msg in public_dkg_shares.values().cloned() {
            let packet = wsts::net::Packet {
                msg: wsts::net::Message::DkgPublicShares(msg),
                sig: Vec::new(),
            };

            // We're in the state that can accept public keys, let's
            // process them.
            coordinator
                .process_message(&packet)
                .map_err(coordinator_error)?;
        }

        // Once we've processed all DKG public shares for all participants,
        // WSTS moves the state to `DKG_PRIVATE_DISTRIBUTE` automatically.
        // If this fails then we know that there is a mismatch between the
        // stored public shares and the size of the input `signers`
        // variable.
        debug_assert_eq!(coordinator.0.state, WstsState::DkgPrivateDistribute);

        // Okay we've already gotten the private keys, and we've set the
        // `party_polynomials` variable in the `WstsCoordinator`. Now we
        // can just set the aggregate key and move the state to the `IDLE`,
        // which is the state after a successful DKG round.
        coordinator.set_aggregate_public_key(Some(aggregate_key));

        coordinator
            .move_to(WstsState::Idle)
            .map_err(coordinator_error)?;

        Ok(coordinator)
    }
}

impl std::ops::Deref for CoordinatorStateMachine {
    type Target = WstsCoordinator;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for CoordinatorStateMachine {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Convert an error
pub fn coordinator_error(err: wsts::state_machine::coordinator::Error) -> error::Error {
    error::Error::WstsCoordinator(Box::new(err))
}
