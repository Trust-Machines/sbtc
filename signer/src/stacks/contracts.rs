//! This module contains functionality for creating stacks transactions for
//! sBTC contract calls.
//!
//! Contains structs for the following contract calls:
//! * [`CompleteDepositV1`]: Used for calling the complete-deposit-wrapper
//!   function in the sbtc-deposit contract. This finalizes the deposit by
//!   minting sBTC and sending it to the depositor.
//! * [`AcceptWithdrawalV1`]: Used for calling the
//!   accept-withdrawal-request function in the sbtc-withdrawal contract.
//!   This finalizes the withdrawal request by burning the locked sBTC.
//! * [`RejectWithdrawalV1`]: Used for calling the reject-withdrawal
//!   function in the sbtc-withdrawal contract. This finalizes the
//!   withdrawal request by returning the locked sBTC to the requester.

use bitcoin::hashes::Hash as _;
use bitcoin::OutPoint;
use bitvec::array::BitArray;
use bitvec::field::BitField as _;
use blockstack_lib::chainstate::stacks::TransactionContractCall;
use blockstack_lib::chainstate::stacks::TransactionPostCondition;
use blockstack_lib::chainstate::stacks::TransactionPostConditionMode;
use blockstack_lib::clarity::vm::types::BuffData;
use blockstack_lib::clarity::vm::types::PrincipalData;
use blockstack_lib::clarity::vm::types::SequenceData;
use blockstack_lib::clarity::vm::types::StandardPrincipalData;
use blockstack_lib::clarity::vm::ClarityName;
use blockstack_lib::clarity::vm::ContractName;
use blockstack_lib::clarity::vm::Value;
use blockstack_lib::types::chainstate::StacksAddress;

/// A struct describing any transaction post-execution conditions that we'd
/// like to enforce.
///
/// # Note
///
/// * It's unlikely that this will be necessary since the signers control
///   the contract to begin with, we implicitly trust it.
/// * We cannot enforce any conditions on the destination of any sBTC, just
///   the source and the amount.
/// * SIP-005 describes the post conditions, including its limitations, and
///   can be found here
///   https://github.com/stacksgov/sips/blob/main/sips/sip-005/sip-005-blocks-and-transactions.md#transaction-post-conditions
#[derive(Debug)]
pub struct StacksTxPostConditions {
    /// Specifies whether other asset transfers not covered by the
    /// post-conditions are permitted.
    pub post_condition_mode: TransactionPostConditionMode,
    /// Any post-execution conditions that we'd like to enforce.
    pub post_conditions: Vec<TransactionPostCondition>,
}

/// A trait to ease construction of a StacksTransaction making sBTC related
/// contract calls.
pub trait AsContractCall {
    /// The name of the clarity smart contract that relates to this struct.
    const CONTRACT_NAME: &'static str;
    /// The specific function name that relates to this struct.
    const FUNCTION_NAME: &'static str;
    /// The arguments to the clarity function.
    fn as_contract_args(&self) -> Vec<Value>;
    /// Convert this struct to a Stacks contract call. The deployer is the
    /// stacks address that deployed the contract.
    fn as_contract_call(&self, deployer: StacksAddress) -> TransactionContractCall {
        TransactionContractCall {
            address: deployer,
            // The following From::from calls are more dangerous than they
            // appear. Under the hood they call their TryFrom::try_from
            // implementation and then unwrap them(!). We check that this
            // is fine in our test.
            function_name: ClarityName::from(Self::FUNCTION_NAME),
            contract_name: ContractName::from(Self::CONTRACT_NAME),
            function_args: self.as_contract_args(),
        }
    }
    /// Any post-execution conditions that we'd like to enforce. The
    /// deployer corresponds to the principal in the Transaction
    /// post-conditions, which is the address that sent the asset. The
    /// default is that we do not enforce any conditions since we usually
    /// deployed the contract.
    fn post_conditions(&self, _: StacksAddress) -> StacksTxPostConditions {
        StacksTxPostConditions {
            post_condition_mode: TransactionPostConditionMode::Allow,
            post_conditions: Vec::new(),
        }
    }
}

/// This struct is used to generate a properly formatted Stacks transaction
/// for calling the complete-deposit-wrapper function in the sbtc-deposit
/// smart contract.
#[derive(Copy, Clone, Debug)]
pub struct CompleteDepositV1 {
    /// The outpoint of the bitcoin UTXO that was spent as a deposit for
    /// sBTC.
    pub outpoint: OutPoint,
    /// The amount of sats associated with the above UTXO.
    pub amount: u64,
    /// The address where the newly minted sBTC will be deposited.
    pub recipient: StacksAddress,
}

impl AsContractCall for CompleteDepositV1 {
    const CONTRACT_NAME: &'static str = "sbtc-deposit";
    const FUNCTION_NAME: &'static str = "complete-deposit-wrapper";
    /// Construct the input arguments to the complete-deposit-wrapper
    /// contract call.
    fn as_contract_args(&self) -> Vec<Value> {
        let txid_data = self.outpoint.txid.to_byte_array().to_vec();
        let txid = BuffData { data: txid_data };
        let principle = StandardPrincipalData::from(self.recipient);

        vec![
            Value::Sequence(SequenceData::Buffer(txid)),
            Value::UInt(self.outpoint.vout as u128),
            Value::UInt(self.amount as u128),
            Value::Principal(PrincipalData::Standard(principle)),
        ]
    }
}

/// This struct is used to generate a properly formatted Stacks transaction
/// for calling the accept-withdrawal-request function in the
/// sbtc-withdrawal smart contract.
#[derive(Copy, Clone, Debug)]
pub struct AcceptWithdrawalV1 {
    /// The ID of the withdrawal request generated by the
    /// initiate-withdrawal-request function in the sbtc-withdrawal smart
    /// contract.
    pub request_id: u64,
    /// The outpoint of the bitcoin UTXO that was spent to fulfill the
    /// withdrawal request.
    pub outpoint: OutPoint,
    /// The fee that was spent to the bitcoin miner when fulfilling the
    /// withdrawal request.
    pub tx_fee: u64,
    /// A bitmap of how the signers voted. This structure supports up to
    /// 128 distinct signers. Here, we assume that a 1 (or true) implies
    /// that the signer voted *against* the transaction.
    pub signer_bitmap: BitArray<[u64; 2]>,
}

impl AsContractCall for AcceptWithdrawalV1 {
    const CONTRACT_NAME: &'static str = "sbtc-withdrawal";
    const FUNCTION_NAME: &'static str = "accept-withdrawal-request";

    fn as_contract_args(&self) -> Vec<Value> {
        let txid_data = self.outpoint.txid.to_byte_array().to_vec();
        let txid = BuffData { data: txid_data };

        vec![
            Value::UInt(self.request_id as u128),
            Value::Sequence(SequenceData::Buffer(txid)),
            Value::UInt(self.outpoint.vout as u128),
            Value::UInt(self.signer_bitmap.load()),
            Value::UInt(self.outpoint.vout as u128),
            Value::UInt(self.tx_fee as u128),
        ]
    }
}

/// This struct is used to generate a properly formatted Stacks transaction
/// for calling the reject-withdrawal function in the sbtc-withdrawal smart
/// contract.
#[derive(Copy, Clone, Debug)]
pub struct RejectWithdrawalV1 {
    /// The ID of the withdrawal request generated by the
    /// initiate-withdrawal-request function in the sbtc-withdrawal smart
    /// contract.
    pub request_id: u64,
    /// A bitmap of how the signers voted. This structure supports up to
    /// 128 distinct signers. Here, we assume that a 1 (or true) implies
    /// that the signer voted *against* the transaction.
    pub signer_bitmap: BitArray<[u64; 2]>,
}

impl AsContractCall for RejectWithdrawalV1 {
    const CONTRACT_NAME: &'static str = "sbtc-withdrawal";
    const FUNCTION_NAME: &'static str = "reject-withdrawal";

    fn as_contract_args(&self) -> Vec<Value> {
        vec![
            Value::UInt(self.request_id as u128),
            Value::UInt(self.signer_bitmap.load()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deposit_contract_call_creation() {
        // This is to check that this function doesn't implicitly panic. If
        // it doesn't panic now, it can never panic at runtime.
        let call = CompleteDepositV1 {
            outpoint: OutPoint::null(),
            amount: 15000,
            recipient: StacksAddress::burn_address(true),
        };

        let _ = call.as_contract_call(StacksAddress::burn_address(false));
    }

    #[test]
    fn withdrawal_accept_contract_call_creation() {
        // This is to check that this function doesn't implicitly panic. If
        // it doesn't panic now, it can never panic at runtime.
        let call = AcceptWithdrawalV1 {
            request_id: 42,
            outpoint: OutPoint::null(),
            tx_fee: 125,
            signer_bitmap: BitArray::new([0; 2]),
        };

        let _ = call.as_contract_call(StacksAddress::burn_address(false));
    }

    #[test]
    fn reject_withdrawal_contract_call_creation() {
        // This is to check that this function doesn't implicitly panic. If
        // it doesn't panic now, it can never panic at runtime.
        let call = RejectWithdrawalV1 {
            request_id: 42,
            signer_bitmap: BitArray::new([1; 2]),
        };

        let _ = call.as_contract_call(StacksAddress::burn_address(false));
    }
}
