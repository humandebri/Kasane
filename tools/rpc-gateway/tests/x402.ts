// where: x402 facilitator tests / what: checks supported route and ERC-3009 calldata / why: protect x402 wire compatibility

import assert from "node:assert/strict";
import { Wallet } from "ethers";
import { configureGateway } from "../src/config.js";
import { handleRpcHttp } from "../src/http.js";
import { receiveWithAuthorizationCalldata, settlePayment } from "../src/x402.js";

const SETTLER_KEY = "0x59c6995e998f97a5a0044966f094538e9d413fd358ee7749852fcb4ec6b9d6ec";

export async function testX402(): Promise<void> {
  configureGateway({
    EVM_CANISTER_ID: "aaaaa-aa",
    X402_NETWORK: "eip155:4801360",
  });

  const supported = await handleRpcHttp({
    method: "GET",
    path: "/v2/x402/supported",
    origin: undefined,
    readBodyText: async () => "",
  });
  assert.equal(supported.status, 200);
  assert.ok(supported.body);
  assert.deepEqual(JSON.parse(supported.body), {
    kinds: [{ x402Version: 2, scheme: "exact", network: "eip155:4801360" }],
    extensions: {},
  });

  const calldata = receiveWithAuthorizationCalldata({
    x402Version: 2,
    accepted: {
      scheme: "exact",
      network: "eip155:4801360",
      asset: "0x1111111111111111111111111111111111111111",
      amount: "1000",
      payTo: "0x2222222222222222222222222222222222222222",
      maxTimeoutSeconds: 60,
      extra: { name: "Kasane Wrapped 01020304", version: "1" },
    },
    payload: {
      signature: `0x${"11".repeat(65)}`,
      authorization: {
        from: "0x3333333333333333333333333333333333333333",
        to: "0x2222222222222222222222222222222222222222",
        value: "1000",
        validAfter: "1",
        validBefore: "9999999999",
        nonce: `0x${"44".repeat(32)}`,
      },
    },
  });
  assert.equal(calldata.slice(0, 10), "0xef55bec6");

  configureGateway({
    EVM_CANISTER_ID: "aaaaa-aa",
    X402_NETWORK: "eip155:4801360",
  });
  const missingKey = await settlePayment(makePaymentBody("0x2222222222222222222222222222222222222222"));
  assert.equal(missingKey.success, false);
  assert.equal(missingKey.errorReason, "settle_exact_node_failure");

  const futureWindow = makePaymentBody("0x2222222222222222222222222222222222222222");
  futureWindow.paymentPayload.payload.authorization.validAfter = String(Math.floor(Date.now() / 1000) + 600);
  const futureWindowMissingKey = await settlePayment(futureWindow);
  assert.equal(futureWindowMissingKey.success, false);
  assert.equal(futureWindowMissingKey.errorReason, "settle_exact_node_failure");

  const expiredWindow = makePaymentBody("0x2222222222222222222222222222222222222222");
  expiredWindow.paymentPayload.payload.authorization.validBefore = String(Math.floor(Date.now() / 1000) - 1);
  const expiredWindowMissingKey = await settlePayment(expiredWindow);
  assert.equal(expiredWindowMissingKey.success, false);
  assert.equal(expiredWindowMissingKey.errorReason, "settle_exact_node_failure");

  configureGateway({
    EVM_CANISTER_ID: "aaaaa-aa",
    X402_NETWORK: "eip155:4801360",
    X402_SETTLER_PRIVATE_KEY: SETTLER_KEY,
  });
  const mismatch = await settlePayment(makePaymentBody("0x2222222222222222222222222222222222222222"));
  assert.equal(mismatch.success, false);
  assert.equal(mismatch.errorReason, "settle_exact_evm_pay_to_mismatch");
  assert.equal(mismatch.transaction, `0x${"0".repeat(64)}`);

  const settlerAddress = new Wallet(SETTLER_KEY).address;
  assert.notEqual(settlerAddress.toLowerCase(), "0x2222222222222222222222222222222222222222");
}

function makePaymentBody(payTo: string) {
  const now = Math.floor(Date.now() / 1000);
  return {
    x402Version: 2,
    paymentPayload: {
      x402Version: 2,
      accepted: {
        scheme: "exact",
        network: "eip155:4801360",
        asset: "0x1111111111111111111111111111111111111111",
        amount: "1000",
        payTo,
        maxTimeoutSeconds: 60,
        extra: { name: "Kasane Wrapped 01020304", version: "1" },
      },
      payload: {
        signature: `0x${"11".repeat(65)}`,
        authorization: {
          from: "0x3333333333333333333333333333333333333333",
          to: payTo,
          value: "1000",
          validAfter: String(now - 1),
          validBefore: String(now + 600),
          nonce: `0x${"44".repeat(32)}`,
        },
      },
    },
    paymentRequirements: {
      scheme: "exact",
      network: "eip155:4801360",
      asset: "0x1111111111111111111111111111111111111111",
      amount: "1000",
      payTo,
      maxTimeoutSeconds: 60,
      extra: { name: "Kasane Wrapped 01020304", version: "1" },
    },
  };
}
