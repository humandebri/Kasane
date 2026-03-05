// どこで: wrapperテスト / 何を: 主要ロジックのユニットテストを実行 / なぜ: request_id導出・状態統合・execution参照の退行を防ぐため

import assert from "node:assert/strict";
import { mergeStatus } from "../lib/merge";
import { decimalToBytes32, deriveRequestId, deriveWrapRequestId, encodeUnwrapAbiInput } from "../lib/request-id";
import { principalTextToBytes } from "../lib/principal";
import { bytesToHex, hexToBytes, parseRequestIdHex } from "../lib/utils";
import { getExecutionResult } from "../lib/canister/wrap-client";
import {
  messageAfterRefreshSuccess,
  nextPollFailureState,
  shouldScheduleAutoPolling,
} from "../lib/status-poll";
import {
  computeRequiredAllowances,
  computeWrapFeeQuote,
  deriveStatusPhase,
  isTerminalStatus,
} from "../lib/wrap-flow";

async function runUtilsTests(): Promise<void> {
  const value = Uint8Array.from([0x01, 0xab, 0x10]);
  const hex = bytesToHex(value);
  assert.equal(hex, "0x01ab10");

  const decoded = hexToBytes(hex);
  assert.deepEqual(decoded, value);

  assert.throws(() => hexToBytes("0x0"));
  assert.throws(() => parseRequestIdHex("0x11"));
  assert.doesNotThrow(() => parseRequestIdHex(`0x${"11".repeat(32)}`));
}

async function runRequestIdTests(): Promise<void> {
  const abi = encodeUnwrapAbiInput({
    vaultCanisterId: "2vxsx-fae",
    assetId: "2vxsx-fae",
    amount: 1n,
    recipient: "2vxsx-fae",
    userNonce: 1n,
    deadline: 2n,
  });
  assert.equal(abi.length % 32, 0);

  const requestId = deriveRequestId({
    callerEvmAddress: Uint8Array.from(new Array(20).fill(0x11)),
    vaultCanisterId: "2vxsx-fae",
    assetId: "2vxsx-fae",
    amount: 1n,
    recipient: "2vxsx-fae",
    userNonce: 1n,
    deadline: 2n,
  });
  assert.equal(requestId.length, 32);

  const wrapRequestId = deriveWrapRequestId({
    // 匿名principalではなく、実運用で使う非匿名principal bytesを使う。
    fromOwner: principalTextToBytes("4c52m-aiaaa-aaaam-agwwa-cai"),
    assetId: principalTextToBytes("2vxsx-fae"),
    amount: decimalToBytes32("1000000000000000000"),
    evmRecipient: hexToBytes("0x1111111111111111111111111111111111111111"),
    evmNonce: 1n,
    gasLimit: 300_000n,
  });
  assert.equal(wrapRequestId.length, 32);
}

async function runMergeTests(): Promise<void> {
  const merged = mergeStatus({
    requestIdHex: `0x${"11".repeat(32)}`,
    dispatchStatus: "Queued",
    dispatchResult: {
      status: "Dispatched",
      vaultCanisterId: Uint8Array.from([4]),
      errorCode: null,
    },
    executionResult: {
      status: "Succeeded",
      ledgerTxId: Uint8Array.from([9, 8]),
      errorCode: null,
      mintFailedRecoverable: false,
      withdrawn: false,
      withdrawLedgerTxId: null,
      withdrawErrorCode: null,
    },
  });
  assert.equal(merged.dispatchStatus, "Dispatched");
  assert.equal(merged.executionStatus, "Succeeded");
  assert.equal(merged.ledgerTxId, "0x0908");
  assert.equal(merged.withdrawn, false);
}

async function runExecutionBranchTests(): Promise<void> {
  const requestId = Uint8Array.from(new Array(32).fill(0x11));

  const unwrapOnly = await getExecutionResult(requestId, {
    readUnwrapResult: async () => [{
      status: { Succeeded: null },
      ledger_tx_id: [Uint8Array.from([0xaa, 0xbb])],
      error_code: [],
    }],
    readWrapResult: async () => [],
  });
  assert.equal(unwrapOnly?.status, "Succeeded");
  assert.deepEqual(unwrapOnly?.ledgerTxId, Uint8Array.from([0xaa, 0xbb]));

  const wrapPreferred = await getExecutionResult(requestId, {
    readUnwrapResult: async () => [{
      status: { Failed: null },
      ledger_tx_id: [],
      error_code: ["unwrap_failed"],
    }],
    readWrapResult: async () => [{
      status: { Failed: null },
      pull_ledger_tx_id: [Uint8Array.from([0x01])],
      mint_tx_id: [],
      error_code: ["wrap_failed"],
      withdrawn: false,
      withdraw_ledger_tx_id: [],
      withdraw_error_code: [],
      mint_failed_recoverable: true,
      fee_ledger_tx_id: [],
      charged_fee_e8s: [],
      charged_gas_price_wei: [],
    }],
  });
  assert.equal(wrapPreferred?.errorCode, "wrap_failed");
  assert.equal(wrapPreferred?.mintFailedRecoverable, true);
}

async function runFeeQuoteMathTests(): Promise<void> {
  const quote = computeWrapFeeQuote({
    gasPriceWei: 250_000_000_000n,
    gasLimit: 300_000n,
    cycleFeeE8s: 1_000_000n,
    gasPriceBufferBps: 12_000n,
  });
  assert.equal(quote.chargedGasPriceWei, 300_000_000_000n);
  assert.equal(quote.totalFeeE8s, 10_000_000n);
}

async function runAllowanceTests(): Promise<void> {
  const separate = computeRequiredAllowances({
    assetLedgerCanister: "a",
    feeLedgerCanister: "b",
    amount: 200n,
    totalFeeE8s: 50n,
  });
  assert.equal(separate.requiredAssetAllowance, 200n);
  assert.equal(separate.requiredFeeAllowance, 50n);

  const merged = computeRequiredAllowances({
    assetLedgerCanister: "a",
    feeLedgerCanister: "a",
    amount: 200n,
    totalFeeE8s: 50n,
  });
  assert.equal(merged.requiredAssetAllowance, 250n);
  assert.equal(merged.requiredFeeAllowance, 0n);
}

async function runStatusPhaseTests(): Promise<void> {
  assert.equal(deriveStatusPhase(null), "idle");
  assert.equal(
    deriveStatusPhase({
      requestId: "0x11",
      dispatchStatus: "Queued",
      executionStatus: null,
      vaultCanisterId: null,
      ledgerTxId: null,
      errorCode: null,
      mintFailedRecoverable: false,
      withdrawn: false,
      withdrawLedgerTxId: null,
      withdrawErrorCode: null,
    }),
    "submitted",
  );
  assert.equal(
    deriveStatusPhase({
      requestId: "0x11",
      dispatchStatus: "Dispatched",
      executionStatus: "Running",
      vaultCanisterId: null,
      ledgerTxId: null,
      errorCode: null,
      mintFailedRecoverable: false,
      withdrawn: false,
      withdrawLedgerTxId: null,
      withdrawErrorCode: null,
    }),
    "executing",
  );
  assert.equal(
    isTerminalStatus({
      requestId: "0x11",
      dispatchStatus: "Dispatched",
      executionStatus: "Succeeded",
      vaultCanisterId: null,
      ledgerTxId: null,
      errorCode: null,
      mintFailedRecoverable: false,
      withdrawn: false,
      withdrawLedgerTxId: null,
      withdrawErrorCode: null,
    }),
    true,
  );
  assert.equal(
    isTerminalStatus({
      requestId: "0x11",
      dispatchStatus: "Dispatching",
      executionStatus: "Running",
      vaultCanisterId: null,
      ledgerTxId: null,
      errorCode: null,
      mintFailedRecoverable: false,
      withdrawn: false,
      withdrawLedgerTxId: null,
      withdrawErrorCode: null,
    }),
    false,
  );
}

async function runStatusPollingRegressionTests(): Promise<void> {
  const nonTerminalStatus = {
    requestId: "0x11",
    dispatchStatus: "Dispatching" as const,
    executionStatus: "Running" as const,
    vaultCanisterId: null,
    ledgerTxId: null,
    errorCode: null,
    mintFailedRecoverable: false,
    withdrawn: false,
    withdrawLedgerTxId: null,
    withdrawErrorCode: null,
  };
  assert.equal(
    shouldScheduleAutoPolling({
      autoPolling: true,
      status: nonTerminalStatus,
      pollFailureCount: 0,
    }),
    true,
  );
  assert.equal(
    shouldScheduleAutoPolling({
      autoPolling: true,
      status: nonTerminalStatus,
      pollFailureCount: 3,
    }),
    false,
  );
  assert.equal(
    shouldScheduleAutoPolling({
      autoPolling: true,
      status: {
        ...nonTerminalStatus,
        executionStatus: "Succeeded",
      },
      pollFailureCount: 0,
    }),
    false,
  );

  const firstFailure = nextPollFailureState({ currentFailureCount: 0 });
  assert.equal(firstFailure.nextFailureCount, 1);
  assert.equal(firstFailure.shouldStop, false);

  const thirdFailure = nextPollFailureState({ currentFailureCount: 2 });
  assert.equal(thirdFailure.nextFailureCount, 3);
  assert.equal(thirdFailure.shouldStop, true);

  assert.equal(
    messageAfterRefreshSuccess({
      currentMessage: "status.auto_poll_stopped",
      background: true,
    }),
    null,
  );
  assert.equal(
    messageAfterRefreshSuccess({
      currentMessage: "wallet.not_connected",
      background: true,
    }),
    "wallet.not_connected",
  );
  assert.equal(
    messageAfterRefreshSuccess({
      currentMessage: "status.auto_poll_stopped",
      background: false,
    }),
    null,
  );
}

async function main(): Promise<void> {
  await runUtilsTests();
  await runRequestIdTests();
  await runMergeTests();
  await runExecutionBranchTests();
  await runFeeQuoteMathTests();
  await runAllowanceTests();
  await runStatusPhaseTests();
  await runStatusPollingRegressionTests();
  process.stdout.write("wrapper tests passed\n");
}

main().catch((error: unknown) => {
  process.stderr.write(`${error instanceof Error ? error.stack ?? error.message : String(error)}\n`);
  process.exitCode = 1;
});
