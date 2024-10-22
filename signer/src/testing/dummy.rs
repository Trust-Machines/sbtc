//! Utilities for generating dummy values on external types

use std::collections::BTreeMap;
use std::ops::Range;

use bitcoin::consensus::Encodable as _;
use bitcoin::hashes::Hash as _;
use bitcoin::Amount;
use bitcoin::OutPoint;
use bitcoin::ScriptBuf;
use bitcoin::TxIn;
use bitcoin::TxOut;
use bitvec::array::BitArray;
use blockstack_lib::burnchains::Txid as StacksTxid;
use blockstack_lib::chainstate::{nakamoto, stacks};
use fake::Fake;
use rand::seq::IteratorRandom as _;
use rand::Rng;
use sbtc::deposits::DepositScriptInputs;
use sbtc::deposits::ReclaimScriptInputs;
use secp256k1::ecdsa::RecoverableSignature;
use secp256k1::SECP256K1;
use stacks_common::types::chainstate::StacksAddress;

use crate::keys::PrivateKey;
use crate::keys::PublicKey;
use crate::keys::SignerScriptPubKey as _;
use crate::stacks::events::CompletedDepositEvent;
use crate::stacks::events::WithdrawalAcceptEvent;
use crate::stacks::events::WithdrawalCreateEvent;
use crate::stacks::events::WithdrawalRejectEvent;
use crate::storage::model;

use crate::codec::Encode;
use crate::storage::model::BitcoinBlockHash;
use crate::storage::model::BitcoinTx;
use crate::storage::model::BitcoinTxId;
use crate::storage::model::EncryptedDkgShares;
use crate::storage::model::RotateKeysTransaction;
use crate::storage::model::ScriptPubKey;
use crate::storage::model::StacksBlockHash;
use crate::storage::model::StacksPrincipal;
use crate::storage::model::StacksTxId;

/// Dummy block
pub fn block<R: rand::RngCore + ?Sized>(config: &fake::Faker, rng: &mut R) -> bitcoin::Block {
    let max_number_of_transactions = 20;

    let number_of_transactions = (rng.next_u32() % max_number_of_transactions) as usize;

    let mut txdata: Vec<bitcoin::Transaction> = std::iter::repeat_with(|| tx(config, rng))
        .take(number_of_transactions)
        .collect();

    txdata.insert(0, coinbase_tx(config, rng));

    let header = bitcoin::block::Header {
        version: bitcoin::block::Version::TWO,
        prev_blockhash: block_hash(config, rng),
        merkle_root: merkle_root(config, rng),
        time: config.fake_with_rng(rng),
        bits: bitcoin::CompactTarget::from_consensus(config.fake_with_rng(rng)),
        nonce: config.fake_with_rng(rng),
    };

    bitcoin::Block { header, txdata }
}

/// Dummy txid
pub fn txid<R: rand::RngCore + ?Sized>(config: &fake::Faker, rng: &mut R) -> bitcoin::Txid {
    let bytes: [u8; 32] = config.fake_with_rng(rng);
    bitcoin::Txid::from_byte_array(bytes)
}

/// Dummy transaction
pub fn tx<R: rand::RngCore + ?Sized>(config: &fake::Faker, rng: &mut R) -> bitcoin::Transaction {
    let max_input_size = 50;
    let max_output_size = 50;

    let input_size = (rng.next_u32() % max_input_size) as usize;
    let output_size = (rng.next_u32() % max_output_size) as usize;

    let input = std::iter::repeat_with(|| txin(config, rng))
        .take(input_size)
        .collect();
    let output = std::iter::repeat_with(|| txout(config, rng))
        .take(output_size)
        .collect();

    bitcoin::Transaction {
        version: bitcoin::transaction::Version::ONE,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input,
        output,
    }
}

/// Dummy transaction input
pub fn txin<R: rand::RngCore + ?Sized>(config: &fake::Faker, rng: &mut R) -> bitcoin::TxIn {
    bitcoin::TxIn {
        previous_output: bitcoin::OutPoint::new(txid(config, rng), config.fake_with_rng(rng)),
        sequence: bitcoin::Sequence::ZERO,
        script_sig: bitcoin::ScriptBuf::new(),
        witness: bitcoin::witness::Witness::new(),
    }
}

/// Dummy transaction output
pub fn txout<R: rand::RngCore + ?Sized>(config: &fake::Faker, rng: &mut R) -> bitcoin::TxOut {
    bitcoin::TxOut {
        value: bitcoin::Amount::from_sat(config.fake_with_rng(rng)),
        script_pubkey: bitcoin::ScriptBuf::new(),
    }
}

/// Dummy block hash
pub fn block_hash<R: rand::RngCore + ?Sized>(
    config: &fake::Faker,
    rng: &mut R,
) -> bitcoin::BlockHash {
    bitcoin::BlockHash::from_byte_array(config.fake_with_rng(rng))
}

/// Dummy merkle root
pub fn merkle_root<R: rand::RngCore + ?Sized>(
    config: &fake::Faker,
    rng: &mut R,
) -> bitcoin::TxMerkleNode {
    bitcoin::TxMerkleNode::from_byte_array(config.fake_with_rng(rng))
}

/// Dummy stacks block
pub fn stacks_block<R: rand::RngCore + ?Sized>(
    config: &fake::Faker,
    rng: &mut R,
) -> nakamoto::NakamotoBlock {
    let max_number_of_transactions = 20;

    let number_of_transactions = (rng.next_u32() % max_number_of_transactions) as usize;

    let txs = std::iter::repeat_with(|| stacks_tx(config, rng))
        .take(number_of_transactions)
        .collect();

    let header = nakamoto::NakamotoBlockHeader::empty();

    nakamoto::NakamotoBlock { header, txs }
}

/// Dummy stacks transaction
pub fn stacks_tx<R: rand::RngCore + ?Sized>(
    config: &fake::Faker,
    rng: &mut R,
) -> stacks::StacksTransaction {
    stacks::StacksTransaction {
        version: stacks::TransactionVersion::Testnet,
        chain_id: config.fake_with_rng(rng),
        auth: stacks::TransactionAuth::from_p2sh(&[], 0).unwrap(),
        anchor_mode: stacks::TransactionAnchorMode::Any,
        post_condition_mode: stacks::TransactionPostConditionMode::Allow,
        post_conditions: Vec::new(),
        payload: stacks::TransactionPayload::new_smart_contract(
            fake::faker::name::en::FirstName().fake_with_rng(rng),
            fake::faker::lorem::en::Paragraph(3..5)
                .fake_with_rng::<String, _>(rng)
                .as_str(),
            None,
        )
        .unwrap(),
    }
}

/// Dummy stacks transaction ID
pub fn stacks_txid<R: rand::RngCore + ?Sized>(
    config: &fake::Faker,
    rng: &mut R,
) -> blockstack_lib::burnchains::Txid {
    blockstack_lib::burnchains::Txid(config.fake_with_rng(rng))
}

/// Dummy signature
pub fn recoverable_signature<R>(config: &fake::Faker, rng: &mut R) -> RecoverableSignature
where
    R: rand::RngCore + ?Sized,
{
    // Represent the signed message.
    let digest: [u8; 32] = config.fake_with_rng(rng);
    let msg = secp256k1::Message::from_digest(digest);
    PrivateKey::new(rng).sign_ecdsa_recoverable(&msg)
}

/// Encrypted dummy DKG shares
pub fn encrypted_dkg_shares<R: rand::RngCore + rand::CryptoRng>(
    _config: &fake::Faker,
    rng: &mut R,
    signer_private_key: &[u8; 32],
    group_key: PublicKey,
) -> model::EncryptedDkgShares {
    let party_state = wsts::traits::PartyState {
        polynomial: None,
        private_keys: vec![],
        nonce: wsts::common::Nonce::random(rng),
    };

    let signer_state = wsts::traits::SignerState {
        id: 0,
        key_ids: vec![1],
        num_keys: 1,
        num_parties: 1,
        threshold: 1,
        group_key: group_key.into(),
        parties: vec![(0, party_state)],
    };

    let encoded = signer_state
        .encode_to_vec()
        .expect("encoding to vec failed");

    let encrypted_private_shares =
        wsts::util::encrypt(signer_private_key, &encoded, rng).expect("failed to encrypt");
    let public_shares: BTreeMap<u32, wsts::net::DkgPublicShares> = BTreeMap::new();
    let public_shares = public_shares
        .encode_to_vec()
        .expect("encoding to vec failed");

    model::EncryptedDkgShares {
        aggregate_key: group_key,
        encrypted_private_shares,
        public_shares,
        tweaked_aggregate_key: group_key.signers_tweaked_pubkey().unwrap(),
        script_pubkey: group_key.signers_script_pubkey().into(),
        signer_set_public_keys: vec![fake::Faker.fake_with_rng(rng)],
        signature_share_threshold: 1,
    }
}

/// Coinbase transaction with random block height
fn coinbase_tx<R: rand::RngCore + ?Sized>(
    config: &fake::Faker,
    rng: &mut R,
) -> bitcoin::Transaction {
    // Numbers below 17 are encoded differently which messes with the block height decoding
    let min_block_height = 17;
    let max_block_height = 10000;
    let block_height = rng.gen_range(min_block_height..max_block_height);
    let coinbase_script = bitcoin::script::Builder::new()
        .push_int(block_height)
        .into_script();

    let mut coinbase_tx = tx(config, rng);
    let mut coinbase_input = txin(config, rng);
    coinbase_input.script_sig = coinbase_script;
    coinbase_tx.input = vec![coinbase_input];

    coinbase_tx
}

impl fake::Dummy<fake::Faker> for PublicKey {
    fn dummy_with_rng<R: rand::Rng + ?Sized>(_: &fake::Faker, rng: &mut R) -> Self {
        let sk = secp256k1::SecretKey::new(rng);
        Self::from(secp256k1::PublicKey::from_secret_key_global(&sk))
    }
}

/// Used to for fine-grained control of generating fake testing addresses.
#[derive(Debug)]
pub struct BitcoinAddresses(pub Range<usize>);

impl fake::Dummy<BitcoinAddresses> for Vec<ScriptPubKey> {
    fn dummy_with_rng<R: rand::Rng + ?Sized>(config: &BitcoinAddresses, rng: &mut R) -> Self {
        let num_addresses = config.0.clone().choose(rng).unwrap_or(1);
        std::iter::repeat_with(|| fake::Faker.fake_with_rng(rng))
            .take(num_addresses)
            .collect()
    }
}

impl fake::Dummy<fake::Faker> for WithdrawalAcceptEvent {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &fake::Faker, rng: &mut R) -> Self {
        let bitmap = rng.next_u64() as u128;
        WithdrawalAcceptEvent {
            txid: blockstack_lib::burnchains::Txid(config.fake_with_rng(rng)),
            block_id: stacks_common::types::chainstate::StacksBlockId(config.fake_with_rng(rng)),
            request_id: rng.next_u32() as u64,
            signer_bitmap: BitArray::new(bitmap.to_le_bytes()),
            outpoint: OutPoint {
                txid: txid(config, rng),
                vout: rng.next_u32(),
            },
            fee: rng.next_u32() as u64,
        }
    }
}

impl fake::Dummy<fake::Faker> for WithdrawalRejectEvent {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &fake::Faker, rng: &mut R) -> Self {
        let bitmap = rng.next_u64() as u128;
        WithdrawalRejectEvent {
            txid: blockstack_lib::burnchains::Txid(config.fake_with_rng(rng)),
            block_id: stacks_common::types::chainstate::StacksBlockId(config.fake_with_rng(rng)),
            request_id: rng.next_u32() as u64,
            signer_bitmap: BitArray::new(bitmap.to_le_bytes()),
        }
    }
}

impl fake::Dummy<fake::Faker> for WithdrawalCreateEvent {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &fake::Faker, rng: &mut R) -> Self {
        WithdrawalCreateEvent {
            txid: StacksTxid(config.fake_with_rng(rng)),
            block_id: stacks_common::types::chainstate::StacksBlockId(config.fake_with_rng(rng)),
            request_id: rng.next_u32() as u64,
            amount: rng.next_u32() as u64,
            sender: config.fake_with_rng::<StacksPrincipal, _>(rng).into(),
            recipient: config.fake_with_rng::<ScriptPubKey, _>(rng).into(),
            max_fee: rng.next_u32() as u64,
            block_height: rng.next_u32() as u64,
        }
    }
}

impl fake::Dummy<fake::Faker> for CompletedDepositEvent {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &fake::Faker, rng: &mut R) -> Self {
        CompletedDepositEvent {
            txid: blockstack_lib::burnchains::Txid(config.fake_with_rng(rng)),
            block_id: stacks_common::types::chainstate::StacksBlockId(config.fake_with_rng(rng)),
            outpoint: OutPoint {
                txid: txid(config, rng),
                vout: rng.next_u32(),
            },
            amount: rng.next_u32() as u64,
        }
    }
}

/// A struct for configuring the signing set of a randomly generated
/// [`RotateKeysTransaction`] that has an aggregate key formed from the
/// randomly generated public keys.
pub struct SignerSetConfig {
    /// The number of signers in the signing set.
    pub num_keys: u16,
    /// The number of signatures required
    pub signatures_required: u16,
}

impl fake::Dummy<SignerSetConfig> for RotateKeysTransaction {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &SignerSetConfig, rng: &mut R) -> Self {
        let signer_set: Vec<PublicKey> = std::iter::repeat_with(|| fake::Faker.fake_with_rng(rng))
            .take(config.num_keys as usize)
            .collect();

        RotateKeysTransaction {
            txid: fake::Faker.fake_with_rng(rng),
            aggregate_key: PublicKey::combine_keys(signer_set.iter()).unwrap(),
            signer_set,
            signatures_required: config.signatures_required,
        }
    }
}

impl fake::Dummy<SignerSetConfig> for EncryptedDkgShares {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &SignerSetConfig, rng: &mut R) -> Self {
        let signer_set_public_keys: Vec<PublicKey> =
            std::iter::repeat_with(|| fake::Faker.fake_with_rng(rng))
                .take(config.num_keys as usize)
                .collect();
        let aggregate_key = PublicKey::combine_keys(&signer_set_public_keys).unwrap();
        EncryptedDkgShares {
            aggregate_key: PublicKey::combine_keys(&signer_set_public_keys).unwrap(),
            tweaked_aggregate_key: aggregate_key.signers_tweaked_pubkey().unwrap(),
            script_pubkey: aggregate_key.signers_script_pubkey().into(),
            encrypted_private_shares: Vec::new(),
            public_shares: Vec::new(),
            signer_set_public_keys,
            signature_share_threshold: config.signatures_required,
        }
    }
}

impl fake::Dummy<fake::Faker> for BitcoinTxId {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &fake::Faker, rng: &mut R) -> Self {
        From::<[u8; 32]>::from(config.fake_with_rng(rng))
    }
}

impl fake::Dummy<fake::Faker> for BitcoinBlockHash {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &fake::Faker, rng: &mut R) -> Self {
        From::<[u8; 32]>::from(config.fake_with_rng(rng))
    }
}

impl fake::Dummy<fake::Faker> for StacksBlockHash {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &fake::Faker, rng: &mut R) -> Self {
        From::<[u8; 32]>::from(config.fake_with_rng(rng))
    }
}

impl fake::Dummy<fake::Faker> for StacksTxId {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &fake::Faker, rng: &mut R) -> Self {
        From::<[u8; 32]>::from(config.fake_with_rng(rng))
    }
}

impl fake::Dummy<fake::Faker> for StacksPrincipal {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &fake::Faker, rng: &mut R) -> Self {
        let public_key: PublicKey = config.fake_with_rng(rng);
        let pubkey = stacks_common::util::secp256k1::Secp256k1PublicKey::from(&public_key);
        let address = StacksAddress::p2pkh(false, &pubkey);
        StacksPrincipal::from(clarity::vm::types::PrincipalData::from(address))
    }
}

impl fake::Dummy<fake::Faker> for ScriptPubKey {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &fake::Faker, rng: &mut R) -> Self {
        let public_key: PublicKey = config.fake_with_rng(rng);
        let pk = bitcoin::CompressedPublicKey(public_key.into());
        let script_pubkey = ScriptBuf::new_p2wpkh(&pk.wpubkey_hash());
        ScriptPubKey::from(script_pubkey)
    }
}

/// A struct to aid in the generation of bitcoin deposit transactions.
///
/// BitcoinTx is created with this config, then it will have a UTXO that is
/// locked with a valid deposit scriptPubKey
#[derive(Debug, Clone, Copy, fake::Dummy)]
pub struct DepositTxConfig {
    /// The public key of the signer.
    pub aggregate_key: PublicKey,
    /// The amount of the deposit
    #[dummy(faker = "2000..1_000_000_000")]
    pub amount: u64,
    /// The max fee of the deposit
    #[dummy(faker = "1000..1_000_000_000")]
    pub max_fee: u64,
    /// The lock-time of the deposit. The value here cannot have the 32nd
    /// bit set to 1 or the else the [`ReclaimScriptInputs::try_new`]
    /// function will return an error.
    #[dummy(faker = "2..250")]
    pub lock_time: i64,
}

impl fake::Dummy<DepositTxConfig> for BitcoinTx {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &DepositTxConfig, rng: &mut R) -> Self {
        let deposit = DepositScriptInputs {
            signers_public_key: config.aggregate_key.into(),
            recipient: fake::Faker.fake_with_rng::<StacksPrincipal, _>(rng).into(),
            max_fee: config.max_fee.min(config.amount),
        };
        let deposit_script = deposit.deposit_script();
        // This is the part of the reclaim script that the user controls.
        let reclaim_script = ScriptBuf::builder()
            .push_opcode(bitcoin::opcodes::all::OP_DROP)
            .push_opcode(bitcoin::opcodes::OP_TRUE)
            .into_script();

        let reclaim = ReclaimScriptInputs::try_new(config.lock_time, reclaim_script).unwrap();
        let reclaim_script = reclaim.reclaim_script();

        let deposit_tx = bitcoin::Transaction {
            version: bitcoin::transaction::Version::TWO,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::null(),
                sequence: bitcoin::Sequence::ZERO,
                script_sig: ScriptBuf::new(),
                witness: bitcoin::Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(config.amount),
                script_pubkey: sbtc::deposits::to_script_pubkey(deposit_script, reclaim_script),
            }],
        };

        Self::from(deposit_tx)
    }
}

impl fake::Dummy<fake::Faker> for BitcoinTx {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &fake::Faker, rng: &mut R) -> Self {
        let deposit_config: DepositTxConfig = config.fake_with_rng(rng);
        deposit_config.fake_with_rng(rng)
    }
}

impl fake::Dummy<DepositTxConfig> for model::Transaction {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &DepositTxConfig, rng: &mut R) -> Self {
        let mut tx = Vec::new();

        let bitcoin_tx: BitcoinTx = config.fake_with_rng(rng);
        bitcoin_tx
            .consensus_encode(&mut tx)
            .expect("In-memory writers never fail");

        model::Transaction {
            tx,
            txid: bitcoin_tx.compute_txid().to_byte_array(),
            tx_type: model::TransactionType::DepositRequest,
            block_hash: fake::Faker.fake_with_rng(rng),
        }
    }
}

/// A struct to aid in the generation of bitcoin sweep transactions.
///
/// BitcoinTx is created with this config, then it will have a UTXO that is
/// locked with a valid scriptPubKey that the signers can spend.
#[derive(Debug, Clone)]
pub struct SweepTxConfig {
    /// The public key of the signers.
    pub aggregate_key: PublicKey,
    /// The amount of the signers UTXO afterwards.
    pub amounts: std::ops::Range<u64>,
    /// The outpoints to use as inputs.
    pub inputs: Vec<OutPoint>,
    /// The outputs to include as withdrawals.
    pub outputs: Vec<(u64, ScriptPubKey)>,
}

impl fake::Dummy<SweepTxConfig> for BitcoinTx {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &SweepTxConfig, rng: &mut R) -> Self {
        let internal_key = config.aggregate_key.into();
        let outpoints = config.inputs.iter().copied();

        let first_output = TxOut {
            value: Amount::from_sat(config.amounts.clone().choose(rng).unwrap_or_default()),
            script_pubkey: ScriptBuf::new_p2tr(SECP256K1, internal_key, None),
        };
        let script_pubkey = if config.outputs.is_empty() {
            ScriptBuf::new_op_return([0; 21])
        } else {
            ScriptBuf::new_op_return([0; 41])
        };
        let second_output = TxOut {
            value: Amount::ZERO,
            script_pubkey,
        };
        let outputs = config.outputs.iter().map(|(amount, script_pub_key)| TxOut {
            value: Amount::from_sat(*amount),
            script_pubkey: script_pub_key.clone().into(),
        });

        let sweep_tx = bitcoin::Transaction {
            version: bitcoin::transaction::Version::TWO,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: outpoints
                .map(|previous_output| TxIn {
                    previous_output,
                    sequence: bitcoin::Sequence::ZERO,
                    script_sig: ScriptBuf::new(),
                    witness: bitcoin::Witness::new(),
                })
                .collect(),
            output: std::iter::once(first_output)
                .chain([second_output])
                .chain(outputs)
                .collect(),
        };

        Self::from(sweep_tx)
    }
}

impl fake::Dummy<SweepTxConfig> for model::Transaction {
    fn dummy_with_rng<R: Rng + ?Sized>(config: &SweepTxConfig, rng: &mut R) -> Self {
        let mut tx = Vec::new();

        let bitcoin_tx: BitcoinTx = config.fake_with_rng(rng);
        bitcoin_tx.consensus_encode(&mut tx).unwrap();

        model::Transaction {
            tx,
            txid: bitcoin_tx.compute_txid().to_byte_array(),
            tx_type: model::TransactionType::SbtcTransaction,
            block_hash: fake::Faker.fake_with_rng(rng),
        }
    }
}
