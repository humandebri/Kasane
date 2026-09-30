import assert from "node:assert/strict";
import { HttpAgent } from "@icp-sdk/core/agent";
import { IDL } from "@icp-sdk/core/candid";
import { Principal } from "@icp-sdk/core/principal";
import type { Channel, Signer } from "@slide-computer/signer";
import { SignerAgent } from "@slide-computer/signer-agent";
import { idlFactory } from "../src/declarations/evm_canister/evm_canister.did.js";
import type { WrapperConfig } from "../lib/config";
import {
  retryFailedUnwrap, retryNativeDeposit, retryNativeWithdrawal,
  submitNativeDeposit, submitWrapRequest, withdrawFailedWrap, wrapClientTestHooks,
} from "../lib/canister/wrap-client";

// Use the installed SignerAgent and actual Candid actor. The wallet rejects before
// execution, so no certificate is fabricated and no HTTP fallback may send funds.
export async function runSignerWrapTests(config: WrapperConfig): Promise<void> {
  const account = Principal.selfAuthenticating(new Uint8Array([1, 2, 3]));
  const calls: Parameters<Signer["callCanister"]>[0][] = [];
  let opened = 0;
  let httpCalls = 0;
  const cancellation = new Error("wallet.user_rejected");
  const channel: Channel = {
    closed: false,
    addEventListener: () => () => {},
    send: async () => { throw new Error("unexpected transport send"); },
    close: async () => {},
  };
  const signer = {
    openChannel: async () => { opened += 1; return channel; },
    supportedStandards: async () => [],
    batchCallCanister: async () => { throw new Error("unexpected batch"); },
    callCanister: async (args: Parameters<Signer["callCanister"]>[0]) => {
      calls.push(args);
      throw cancellation;
    },
  };
  const baseAgent = HttpAgent.createSync({
    host: config.icHost,
    fetch: async () => { httpCalls += 1; throw new Error("unexpected HTTP call"); },
  });
  const agent = await SignerAgent.create({ signer, account, agent: baseAgent, scheduleDelay: 0 });
  const caller = { principalText: account.toText(), cacheKey: "signer-wrap-test", agent };
  const asset = Principal.selfAuthenticating(new Uint8Array([4]));
  const fee = Principal.selfAuthenticating(new Uint8Array([5]));
  const recipient = new Uint8Array(20).fill(0x12);
  const requestId = new Uint8Array(32).fill(0xab);
  const service = idlFactory({ IDL });
  const cases = [
    {
      method: "submit_wrap_request",
      arg: { asset_id: asset, amount_e8s: 123n, evm_recipient: recipient,
        evm_nonce: 7n, gas_limit: 3000000n, max_fee_e8s: 987n,
        quoted_gas_price_wei: 654n, fee_ledger_canister: fee },
      run: () => submitWrapRequest({ assetId: asset.toText(), amountE8s: 123n,
        evmRecipient: recipient, evmNonce: 7n, gasLimit: 3000000n, maxFeeE8s: 987n,
        quotedGasPriceWei: 654n, feeLedgerCanister: fee.toText() }, caller),
    },
    {
      method: "submit_native_deposit",
      arg: { deposit_id: requestId, amount_e8s: 123n, evm_recipient: recipient,
        max_fee_e8s: 987n, fee_ledger_canister: fee },
      run: () => submitNativeDeposit({ depositId: requestId, amountE8s: 123n,
        evmRecipient: recipient, maxFeeE8s: 987n, feeLedgerCanister: fee.toText() }, caller),
    },
    { method: "retry_request", arg: { request_id: requestId }, run: () => retryFailedUnwrap(requestId, caller) },
    { method: "retry_native_deposit", arg: { request_id: requestId }, run: () => retryNativeDeposit(requestId, caller) },
    { method: "retry_native_withdrawal", arg: { request_id: requestId }, run: () => retryNativeWithdrawal(requestId, caller) },
    { method: "recover_failed_wrap", arg: { request_id: requestId }, run: () => withdrawFailedWrap(requestId, caller) },
  ];
  wrapClientTestHooks.reset();
  wrapClientTestHooks.setDeps({ loadConfig: () => config });
  try {
    for (const test of cases) {
      await assert.rejects(test.run, /wallet.user_rejected/);
      const call = calls.at(-1);
      assert.ok(call);
      assert.equal(call.canisterId.toText(), config.wrapCanisterId);
      assert.equal(call.sender.toText(), account.toText());
      assert.equal(call.method, test.method);
      const method = service._fields.find(([name]) => name === test.method)?.[1];
      assert.ok(method);
      assert.deepEqual(call.arg, IDL.encode(method.argTypes, [test.arg]));
    }
    assert.equal(calls.length, cases.length);
    assert.equal(opened, cases.length);
    assert.equal(httpCalls, 0);
  } finally {
    wrapClientTestHooks.reset();
  }
}
