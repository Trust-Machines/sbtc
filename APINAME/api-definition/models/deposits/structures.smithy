$version: "2.0"

namespace stacks.sbtc

resource Deposit {
    identifiers: {
        txid: Base64EncodedBinary
        outputIndex: String
    }
    properties: {
        // These are the same as the identifiers, but are separate
        // to accomodate a Smithy quirk. This should not be a problem
        // in any autogenerated code.
        bitcoinTxid: Base64EncodedBinary
        bitcoinTxOutputIndex: Integer

        // Parameters
        recipient: String
        amount: Satoshis
        lastUpdateHeight: Integer
        lastUpdateBlockHash: Base64EncodedBinary
        status: OpStatus
        statusMessage: String
        parameters: DepositParameters
        fulfillment: Fulfillment

        // Only relevant during creation.
        deposit: Base64EncodedBinary
        reclaim: Base64EncodedBinary
    }
    create: CreateDeposit
    read: GetDeposit
}

structure DepositParameters {
    maxFee: Integer
    lockTime: Integer
    reclaimScript: String
}

structure DepositData {
    // Identifiers
    @required bitcoinTxid: Base64EncodedBinary,
    @required bitcoinTxOutputIndex: Integer,

    // Parameters
    @required recipient: String
    @required amount: Satoshis
    lastUpdateHeight: Integer
    lastUpdateBlockHash: Base64EncodedBinary
    @required status: OpStatus
    @required statusMessage: String
    @required parameters: DepositParameters

    // Fulfilling data.
    fulfillment: Fulfillment
}

list DepositDataList {
    member: DepositData
}

structure DepositBasicInfo {
    @required bitcoinTxid: Base64EncodedBinary
    @required bitcoinTxOutputIndex: Integer
    @required recipient: Base64EncodedBinary
    @required amount: Satoshis
    @required lastUpdateHeight: Integer
    @required lastUpdateBlockHash: Base64EncodedBinary
    @required status: OpStatus
}

list DepositBasicInfoList {
    member: DepositBasicInfo
}

structure DepositUpdate {
    // Identifiers
    @required bitcoinTxid: Base64EncodedBinary,
    @required bitcoinTxOutputIndex: Integer,

    // Parameters
    recipient: String
    amount: Satoshis
    lastUpdateHeight: Integer
    lastUpdateBlockHash: Base64EncodedBinary
    status: OpStatus
    statusMessage: String
    parameters: DepositParameters
    fulfillment: Fulfillment
}

list DepositUpdateList {
    member: DepositUpdate
}