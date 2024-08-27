//! For deconstructing print events emitted from in the sbtc-registry smart
//! contract.
//!
//! Print events from the sBTC registry come in as [`TupleData`] clarity
//! values. Each tuple has a topic field that takes one of four values. We
//! deconstruct the tuple data based on the topic.

use std::collections::BTreeMap;

use bitcoin::hashes::Hash;
use bitcoin::Address;
use bitcoin::OutPoint;
use bitcoin::ScriptBuf;
use bitcoin::Txid;
use bitvec::array::BitArray;
use clarity::vm::types::PrincipalData;
use clarity::vm::types::SequenceData;
use clarity::vm::types::TupleData;
use clarity::vm::ClarityName;
use clarity::vm::Value as ClarityValue;

use crate::config::NetworkKind;
use crate::error::Error;

/// The print events emitted by the sbtc-registry clarity smart contract.
pub enum RegistryEvent {
    /// For the `completed-deposit` topic
    CompletedDeposit(CompletedDepositEvent),
    /// For the `withdrawal-accept` topic
    WithdrawalAccept(WithdrawalAcceptEvent),
    /// For the `withdrawal-reject` topic
    WithdrawalReject(WithdrawalRejectEvent),
    /// For the `withdrawal-create` topic
    WithdrawalCreate(WithdrawalCreateEvent),
}

#[derive(Debug)]
struct RawTupleData(BTreeMap<ClarityName, ClarityValue>);

impl RawTupleData {
    /// Extract the u128 value from the given field
    fn remove_u128(&mut self, field: &'static str) -> Result<u128, Error> {
        match self.0.remove(field) {
            Some(ClarityValue::UInt(val)) => Ok(val),
            _ => Err(Error::TupleEventField(field)),
        }
    }
    /// Extract the buff value from the given field
    fn remove_buff(&mut self, field: &'static str) -> Result<Vec<u8>, Error> {
        match self.0.remove(field) {
            Some(ClarityValue::Sequence(SequenceData::Buffer(buf))) => Ok(buf.data),
            _ => Err(Error::TupleEventField(field)),
        }
    }
    /// Extract the principal value from the given field
    fn remove_principal(&mut self, field: &'static str) -> Result<PrincipalData, Error> {
        match self.0.remove(field) {
            Some(ClarityValue::Principal(principal)) => Ok(principal),
            _ => Err(Error::TupleEventField(field)),
        }
    }
    /// Extract the tuple value from the given field
    fn remove_tuple(&mut self, field: &'static str) -> Result<Self, Error> {
        match self.0.remove(field) {
            Some(ClarityValue::Tuple(TupleData { data_map, .. })) => Ok(Self(data_map)),
            _ => Err(Error::TupleEventField(field)),
        }
    }
}

/// Transform the [`ClarityValue`] from the sbtc-registry event into a
/// proper type.
pub fn deconstruct(value: ClarityValue, network: NetworkKind) -> Result<RegistryEvent, Error> {
    match value {
        ClarityValue::Tuple(TupleData { mut data_map, .. }) => {
            // Lucky for us, each sBTC print event in the sbtc-registry
            // smart contract has a topic. We use that to match on what to
            // expect when decomposing the event from a [`ClarityValue`]
            // into a proper type.
            let topic = match data_map.remove("topic") {
                Some(ClarityValue::Sequence(SequenceData::String(val))) => val.to_string(),
                _ => return Err(Error::TupleEventField("topic")),
            };

            let event_map = RawTupleData(data_map);

            match topic.as_str() {
                "completed-deposit" => completed_deposit(event_map),
                "withdrawal-reject" => withdrawal_reject(event_map),
                "withdrawal-accept" => withdrawal_accept(event_map),
                "withdrawal-create" => withdrawal_create(event_map, network),
                _ => Err(Error::TupleEventField("topic")),
            }
        }
        _ => Err(Error::TupleEventField("topic")),
    }
}

/// This is the event that is emitted from the `create-withdrawal-request`
/// public function in sbtc-registry smart contract.
#[derive(Debug)]
pub struct CompletedDepositEvent {
    /// This is the amount of sBTC to mint to the intended recipient.
    pub amount: u64,
    /// This is the outpoint of the original bitcoin deposit transaction.
    pub outpoint: OutPoint,
}

/// This function if for transforming the print events of the
/// complete-deposit function in the sbtc-registry.
///
/// # Notes
///
/// The print events for complete-deposit calls are structured like so:
///
/// ```clarity
/// (print {
///   topic: "completed-deposit",
///   bitcoin-txid: (buff 32),
///   output-index: uint,
///   amount: uint
/// })
/// ```
///
/// The above event is emitted after the indicated amount of sBTC has been
/// emitted to the recipient.
fn completed_deposit(mut map: RawTupleData) -> Result<RegistryEvent, Error> {
    let amount = map.remove_u128("amount")?;
    let vout = map.remove_u128("output-index")?;
    let txid_bytes = map.remove_buff("bitcoin-txid")?;

    Ok(RegistryEvent::CompletedDeposit(CompletedDepositEvent {
        // This shouldn't error, since this amount is set from the u64
        // amount of sats by us.
        amount: u64::try_from(amount).map_err(Error::ClarityIntConversion)?,
        outpoint: OutPoint {
            // This shouldn't error, this is set from a proper [`Txid`] in
            // a contract call.
            txid: Txid::from_slice(&txid_bytes).map_err(Error::ClarityTxidConversion)?,
            // This shouldn't actually error, we cast u32s to u128s before
            // making the contract call, and that is the value that gets
            // emitted here.
            vout: u32::try_from(vout).map_err(Error::ClarityIntConversion)?,
        },
    }))
}

/// This is the event that is emitted from the `create-withdrawal-request`
/// public function in sbtc-registry smart contract.
#[derive(Debug)]
pub struct WithdrawalCreateEvent {
    /// This is the unique identifier of the withdrawal request.
    pub request_id: u64,
    /// This is the amount of sBTC that is locked and requested to be
    /// withdrawal as sBTC.
    pub amount: u64,
    /// This is the principal who has their sBTC locked up.
    pub sender: PrincipalData,
    /// This is the address to send the BTC to when fulfilling the
    /// withdrawal request.
    pub recipient: Address,
    /// This is the maximum amount of BTC "spent" to the miners for the
    /// transaction fee.
    pub max_fee: u64,
    /// The block height of the bitcoin blockchain when the stacks
    /// transaction that emitted this event was executed.
    pub block_height: u64,
}

/// This function if for transforming the print events of the
/// `complete-withdrawal-accept` function in the sbtc-registry.
///
/// # Notes
///
/// The print events for `create-withdrawal-request` calls are structured
/// like so:
///
/// ```clarity
/// (print {
///   topic: "withdrawal-create",
///   amount: uint,
///   request-id: uint,
///   sender: principal,
///   recipient: { version: (buff 1), hashbytes: (buff 32) },
///   block-height: uint,
///   max-fee: uint,
/// })
/// ```
fn withdrawal_create(mut map: RawTupleData, network: NetworkKind) -> Result<RegistryEvent, Error> {
    let request_id = map.remove_u128("request-id")?;
    let amount = map.remove_u128("amount")?;
    let max_fee = map.remove_u128("max-fee")?;
    let block_height = map.remove_u128("block-height")?;
    let sender = map.remove_principal("sender")?;
    let recipient = map.remove_tuple("recipient")?;

    Ok(RegistryEvent::WithdrawalCreate(WithdrawalCreateEvent {
        // This shouldn't error, practically speaking. Each withdrawal
        // request increments the integer by one, so we'd have to do many
        // orders of magnitude more requests than there are bitcoin
        // transactions, ever.
        request_id: u64::try_from(request_id).map_err(Error::ClarityIntConversion)?,
        amount: u64::try_from(amount).map_err(Error::ClarityIntConversion)?,
        max_fee: u64::try_from(max_fee).map_err(Error::ClarityIntConversion)?,
        block_height: u64::try_from(block_height).map_err(Error::ClarityIntConversion)?,
        recipient: recipient_to_address(recipient, network)?,
        sender,
    }))
}

fn recipient_to_address(_map: RawTupleData, network: NetworkKind) -> Result<Address, Error> {
    Ok(Address::p2shwsh(&ScriptBuf::new_op_return([1, 2]), network))
}


/// This is the event that is emitted from the `complete-withdrawal-accept`
/// public function in sbtc-registry smart contract.
#[derive(Debug)]
pub struct WithdrawalAcceptEvent {
    /// This is the unique identifier of the withdrawal request.
    pub request_id: u64,
    /// The bitmap of how the signers voted for the withdrawal request.
    /// Here, a 1 (or true) implies that the signer did *not* vote to
    /// accept the request.
    pub signer_bitmap: BitArray<[u8; 16]>,
    /// This is the outpoint for the bitcoin transaction that serviced the
    /// request.
    pub outpoint: OutPoint,
    /// This is the fee that was spent to the bitcoin miners to confirm the
    /// withdrawal request.
    pub fee: u64,
}

/// This function if for transforming the print events of the
/// `complete-withdrawal-accept` function in the sbtc-registry.
///
/// # Notes
///
/// The print events for `complete-withdrawal-accept` calls are structured
/// like so:
///
/// ```clarity
/// (print {
///   topic: "withdrawal-accept",
///   request-id: uint,
///   bitcoin-txid: (buff 32),
///   signer-bitmap: uint,
///   bitcoin-index: uint,
///   fee: fee
/// })
/// ```
fn withdrawal_accept(mut map: RawTupleData) -> Result<RegistryEvent, Error> {
    let request_id = map.remove_u128("request-id")?;
    let bitmap = map.remove_u128("signer-bitmap")?;
    let fee = map.remove_u128("fee")?;
    let vout = map.remove_u128("output-index")?;
    let txid_bytes = map.remove_buff("bitcoin-txid")?;

    Ok(RegistryEvent::WithdrawalAccept(WithdrawalAcceptEvent {
        // This shouldn't error for the reasons noted in
        // [`withdrawal_create`].
        request_id: u64::try_from(request_id).map_err(Error::ClarityIntConversion)?,
        signer_bitmap: BitArray::new(bitmap.to_be_bytes()),
        outpoint: OutPoint {
            // This shouldn't error, this is set from a proper [`Txid`] in
            // a contract call.
            txid: Txid::from_slice(&txid_bytes).map_err(Error::ClarityTxidConversion)?,
            // This shouldn't actually error, we cast u32s to u128s before
            // making the contract call, and that is the value that gets
            // emitted here.
            vout: u32::try_from(vout).map_err(Error::ClarityIntConversion)?,
        },
        // This shouldn't error, since this amount is set from the u64
        // amount of sats by us.
        fee: u64::try_from(fee).map_err(Error::ClarityIntConversion)?,
    }))
}

/// This is the event that is emitted from the `complete-withdrawal-reject`
/// public function in sbtc-registry smart contract.
pub struct WithdrawalRejectEvent {
    /// This is the unique identifier of user created the withdrawal
    /// request.
    pub request_id: u64,
    /// The bitmap of how the signers voted for the withdrawal request.
    /// Here, a 1 (or true) implies that the signer did *not* vote to
    /// accept the request.
    pub signer_bitmap: BitArray<[u8; 16]>,
}

/// This function if for transforming the print events of the
/// `complete-withdrawal-reject` function in the sbtc-registry.
///
/// # Notes
///
/// The print events for `complete-withdrawal-reject` calls are structured
/// like so:
///
/// ```clarity
/// (print {
///   topic: "withdrawal-reject",
///   request-id: uint,
///   signer-bitmap: uint,
/// })
/// ```
///
/// The above event is emitted after the locked sBTC has been unlocked back
/// to the account that initiated the request.
fn withdrawal_reject(mut map: RawTupleData) -> Result<RegistryEvent, Error> {
    let request_id = map.remove_u128("request-id")?;
    let bitmap = map.remove_u128("signer-bitmap")?;

    Ok(RegistryEvent::WithdrawalReject(WithdrawalRejectEvent {
        // This shouldn't error for the reasons noted in
        // [`withdrawal_create`].
        request_id: u64::try_from(request_id).map_err(Error::ClarityIntConversion)?,
        signer_bitmap: BitArray::new(bitmap.to_le_bytes()),
    }))
}

#[cfg(test)]
mod tests {
    use bitvec::field::BitField as _;

    use super::*;

    #[test]
    fn signer_bitmap_conversion() {
        // This test checks that converting from an integer to the bitmap
        // works the way that we expect.
        let bitmap_number: u128 = 3;
        let bitmap: BitArray<[u8; 16]> = BitArray::new(bitmap_number.to_le_bytes());

        assert_eq!(bitmap.load_le::<u128>(), bitmap_number);

        // This is basically a test of the same thing as the above, except
        // that we explicitly create the signer bitmap.
        let mut bitmap: BitArray<[u8; 16]> = BitArray::ZERO;
        bitmap.set(0, true);
        bitmap.set(1, true);

        assert_eq!(bitmap.load_le::<u128>(), bitmap_number);
    }
}
