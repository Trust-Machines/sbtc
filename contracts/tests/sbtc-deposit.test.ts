import {
  alice,
  bob,
  deposit,
  getLastWithdrawalRequestId,
  getWithdrawalRequest,
  registry,
  stxAddressToPoxAddress,
} from "./helpers";
import { test, expect, describe } from "vitest";
import { txOk, filterEvents, rov } from "@clarigen/test";
import { CoreNodeEventType, cvToValue } from "@clarigen/core";

const alicePoxAddr = stxAddressToPoxAddress(alice);
const bobPoxAddr = stxAddressToPoxAddress(bob);

describe("sBTC deposit contract", () => {
  describe("complete deposit wrapper placeholder", () => {
    test("Response is Ok", () => {
      const lastId = getLastWithdrawalRequestId();
      expect(lastId).toEqual(0n);
    });

    test("Call complete-deposit-wrapper", () => {
      const recipient = alicePoxAddr;
      const receipt = txOk(
        deposit.createWithdrawalRequest({
          amount: 100n,
          recipient,
          maxFee: 10n,
          sender: alice,
          height: simnet.blockHeight,
        }),
        alice
      );
      expect(receipt.value).toEqual(1n);

      const request = getWithdrawalRequest(1n);
      if (!request) {
        throw new Error("Request not stored");
      }
      expect(request.sender).toEqual(alice);
      expect(request.amount).toEqual(100n);
      expect(request.maxFee).toEqual(10n);
      expect(request.recipient).toEqual(recipient);
      expect(request.blockHeight).toEqual(BigInt(simnet.blockHeight - 1));

      // ensure last-id is updated
      const lastId = getLastWithdrawalRequestId();
      expect(lastId).toEqual(1n);
    });

    test("emitted events when a new withdrawal is stored", () => {
      const recipient = bobPoxAddr;
      const receipt = txOk(
        registry.createWithdrawalRequest({
          recipient,
          amount: 100n,
          maxFee: 10n,
          sender: bob,
          height: simnet.blockHeight,
        }),
        alice
      );
      const prints = filterEvents(
        receipt.events,
        CoreNodeEventType.ContractEvent
      );
      expect(prints.length).toEqual(1);
      const [print] = prints;
      const printData = cvToValue<{
        sender: string;
        recipient: string;
        amount: bigint;
        maxFee: bigint;
        blockHeight: bigint;
        topic: string;
      }>(print.data.value);

      const request = getWithdrawalRequest(1n);
      if (!request) {
        throw new Error("Request not stored");
      }
      expect(printData).toStrictEqual({
        sender: bob,
        recipient: recipient,
        amount: 100n,
        maxFee: 10n,
        blockHeight: request.blockHeight,
        topic: "withdrawal-request",
        requestId: 1n,
      });
    });

    test("get-withdrawal-request includes status", () => {
      txOk(
        registry.createWithdrawalRequest({
          sender: alice,
          recipient: alicePoxAddr,
          amount: 100n,
          maxFee: 10n,
          height: simnet.blockHeight,
        }),
        alice
      );

      const request = rov(registry.getWithdrawalRequest(1n));
      if (!request) {
        throw new Error("Request not found");
      }
      expect(request.status).toEqual(null);
      expect(request).toStrictEqual({
        sender: alice,
        recipient: alicePoxAddr,
        amount: 100n,
        maxFee: 10n,
        blockHeight: BigInt(simnet.blockHeight - 1),
        status: null,
      });
    });
  });
});
