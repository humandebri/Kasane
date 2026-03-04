// どこで: wrapperテスト / 何を: 入力検証・統合ロジック・APIルートを検証 / なぜ: dispatch/execution分離契約の退行を防ぐため

import assert from "node:assert/strict";
import { GET as healthGet } from "../app/api/health/route";
import { POST as submitPost } from "../app/api/wrap/submit/route";
import { GET as statusGet } from "../app/api/wrap/status/[requestId]/route";
import { POST as withdrawPost } from "../app/api/wrap/withdraw/route";
import { submitUnwrapRequest } from "../lib/server";
import { mergeStatus } from "../lib/merge";
import { deriveRequestId, encodeUnwrapAbiInput } from "../lib/request-id";
import { getExecutionResult } from "../lib/canister/wrap-client";
import {
  setHealthDepsOverride,
  setStatusDepsOverride,
  setSubmitDepsOverride,
  setWithdrawDepsOverride,
} from "../lib/route-test-overrides";
import type { DispatchResultView, DispatchStatus, ExecutionStatus } from "../lib/types";
import { assertValidRequestIdHex, parseSubmitPayload } from "../lib/validate";

function setEnv(): void {
  process.env.NEXT_PUBLIC_IC_HOST = "http://127.0.0.1:4943";
  process.env.EVM_GATEWAY_CANISTER_ID = "aaaaa-aa";
  process.env.WRAP_CANISTER_ID = "2vxsx-fae";
  process.env.ICP_IDENTITY_SECRET_KEY_HEX = "11".repeat(32);
  process.env.FETCH_ROOT_KEY = "false";
}

function makeRequestJson(body: unknown): Request {
  return new Request("http://localhost/api", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

async function readJson(response: Response): Promise<unknown> {
  return response.json();
}

async function runValidationTests(): Promise<void> {
  parseSubmitPayload({
    assetId: "2vxsx-fae",
    amount: "100",
    recipient: "2vxsx-fae",
  });

  assert.throws(() =>
    parseSubmitPayload({
      assetId: "bad",
      amount: "100",
      recipient: "2vxsx-fae",
    })
  );

  assert.throws(() =>
    parseSubmitPayload({
      assetId: "2vxsx-fae",
      amount: "0",
      recipient: "2vxsx-fae",
    })
  );

  assert.throws(() =>
    parseSubmitPayload({
      kind: "unwrap",
      assetId: "2vxsx-fae",
      amount: "100",
      recipient: "2vxsx-fae",
    })
  );

  assert.doesNotThrow(() => assertValidRequestIdHex(`0x${"11".repeat(32)}`));
  assert.throws(() => assertValidRequestIdHex("0x11"));
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
}

async function runMergeTests(): Promise<void> {
  const dispatch: DispatchResultView = {
    status: "Dispatched",
    vaultCanisterId: Uint8Array.from([4]),
    errorCode: null,
  };

  const mergedBoth = mergeStatus({
    requestIdHex: `0x${"11".repeat(32)}`,
    dispatchStatus: "Queued",
    dispatchResult: dispatch,
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
  assert.equal(mergedBoth.dispatchStatus, "Dispatched");
  assert.equal(mergedBoth.executionStatus, "Succeeded");
  assert.equal(mergedBoth.ledgerTxId, "0x0908");
  assert.equal(mergedBoth.mintFailedRecoverable, false);
  assert.equal(mergedBoth.withdrawn, false);
  assert.equal(mergedBoth.withdrawLedgerTxId, null);
  assert.equal(mergedBoth.withdrawErrorCode, null);

  const mergedDispatchOnly = mergeStatus({
    requestIdHex: `0x${"22".repeat(32)}`,
    dispatchStatus: "Dispatching",
    dispatchResult: null,
    executionResult: null,
  });
  assert.equal(mergedDispatchOnly.dispatchStatus, "Dispatching");
  assert.equal(mergedDispatchOnly.executionStatus, null);

  const mergedExecutionOnly = mergeStatus({
    requestIdHex: `0x${"33".repeat(32)}`,
    dispatchStatus: null,
    dispatchResult: null,
    executionResult: {
      status: "Failed",
      ledgerTxId: null,
      errorCode: "ledger.transfer_failed:InsufficientFunds",
      mintFailedRecoverable: true,
      withdrawn: false,
      withdrawLedgerTxId: null,
      withdrawErrorCode: "withdraw.call_failed:oops",
    },
  });
  assert.equal(mergedExecutionOnly.dispatchStatus, null);
  assert.equal(mergedExecutionOnly.executionStatus, "Failed");
  assert.equal(mergedExecutionOnly.errorCode, "ledger.transfer_failed:InsufficientFunds");
  assert.equal(mergedExecutionOnly.mintFailedRecoverable, true);
  assert.equal(mergedExecutionOnly.withdrawn, false);
  assert.equal(mergedExecutionOnly.withdrawErrorCode, "withdraw.call_failed:oops");

  const mergedNone = mergeStatus({
    requestIdHex: `0x${"44".repeat(32)}`,
    dispatchStatus: null,
    dispatchResult: null,
    executionResult: null,
  });
  assert.equal(mergedNone.dispatchStatus, null);
  assert.equal(mergedNone.executionStatus, null);
}

async function runWrapClientExecutionTests(): Promise<void> {
  const requestId = Uint8Array.from(Buffer.from("11".repeat(32), "hex"));

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
  assert.equal(unwrapOnly?.mintFailedRecoverable, false);
  assert.equal(unwrapOnly?.withdrawn, false);

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
    }],
  });
  assert.equal(wrapPreferred?.errorCode, "wrap_failed");
  assert.equal(wrapPreferred?.mintFailedRecoverable, true);
}

async function runSubmitApiTests(): Promise<void> {
  setEnv();
  setSubmitDepsOverride({
    readNonce: async () => 0n,
    submitTx: async () => Uint8Array.from([1]),
    readDispatchStatus: async () => "Queued",
    makeUserNonce: () => 42n,
  });

  try {
    const okResponse = await submitPost(
      makeRequestJson({
        assetId: "2vxsx-fae",
        amount: "100",
        recipient: "2vxsx-fae",
      })
    );
    assert.equal(okResponse.status, 200);
    const okBody = (await readJson(okResponse)) as { requestId: string; dispatchStatus: DispatchStatus };
    assert.equal(okBody.dispatchStatus, "Queued");
    assert.equal(okBody.requestId.length, 66);

    const badResponse = await submitPost(
      makeRequestJson({
        assetId: "bad",
        amount: "100",
        recipient: "2vxsx-fae",
      })
    );
    assert.equal(badResponse.status, 400);

    const kindResponse = await submitPost(
      makeRequestJson({
        kind: "unwrap",
        assetId: "2vxsx-fae",
        amount: "100",
        recipient: "2vxsx-fae",
      })
    );
    assert.equal(kindResponse.status, 400);
  } finally {
    setSubmitDepsOverride(null);
  }
}

function buildStatusDeps(args: {
  dispatchStatus: DispatchStatus | null;
  dispatchResult: {
    status: DispatchStatus;
    vaultCanisterId: Uint8Array;
    errorCode: string | null;
  } | null;
  executionResult: {
    status: ExecutionStatus;
    ledgerTxId: Uint8Array | null;
    errorCode: string | null;
    mintFailedRecoverable: boolean;
    withdrawn: boolean;
    withdrawLedgerTxId: Uint8Array | null;
    withdrawErrorCode: string | null;
  } | null;
}) {
  return {
    readDispatchStatus: async () => args.dispatchStatus,
    readDispatchResult: async () => args.dispatchResult,
    readExecutionResult: async () => args.executionResult,
  };
}

async function runStatusApiTests(): Promise<void> {
  setEnv();
  const requestId = `0x${"aa".repeat(32)}`;

  setStatusDepsOverride(
    buildStatusDeps({
      dispatchStatus: "Dispatching",
      dispatchResult: null,
      executionResult: null,
    })
  );
  try {
    const r1 = await statusGet(new Request("http://localhost"), { params: Promise.resolve({ requestId }) });
    assert.equal(r1.status, 200);
    const b1 = (await readJson(r1)) as { dispatchStatus: DispatchStatus | null; executionStatus: ExecutionStatus | null };
    assert.equal(b1.dispatchStatus, "Dispatching");
    assert.equal(b1.executionStatus, null);
  } finally {
    setStatusDepsOverride(null);
  }

  setStatusDepsOverride(
    buildStatusDeps({
      dispatchStatus: null,
      dispatchResult: null,
      executionResult: {
        status: "Failed",
        ledgerTxId: null,
        errorCode: "ledger.transfer_failed:InsufficientFunds",
        mintFailedRecoverable: true,
        withdrawn: false,
        withdrawLedgerTxId: null,
        withdrawErrorCode: null,
      },
    })
  );
  try {
    const r2 = await statusGet(new Request("http://localhost"), { params: Promise.resolve({ requestId }) });
    assert.equal(r2.status, 200);
    const b2 = (await readJson(r2)) as { dispatchStatus: DispatchStatus | null; executionStatus: ExecutionStatus | null };
    assert.equal(b2.dispatchStatus, null);
    assert.equal(b2.executionStatus, "Failed");
  } finally {
    setStatusDepsOverride(null);
  }

  setStatusDepsOverride(
    buildStatusDeps({
      dispatchStatus: "Queued",
      dispatchResult: {
        status: "Dispatched",
        vaultCanisterId: Uint8Array.from([4]),
        errorCode: null,
      },
      executionResult: {
        status: "Running",
        ledgerTxId: null,
        errorCode: null,
        mintFailedRecoverable: false,
        withdrawn: false,
        withdrawLedgerTxId: null,
        withdrawErrorCode: null,
      },
    })
  );
  try {
    const r3 = await statusGet(new Request("http://localhost"), { params: Promise.resolve({ requestId }) });
    assert.equal(r3.status, 200);
    const b3 = (await readJson(r3)) as { dispatchStatus: DispatchStatus | null; executionStatus: ExecutionStatus | null };
    assert.equal(b3.dispatchStatus, "Dispatched");
    assert.equal(b3.executionStatus, "Running");
  } finally {
    setStatusDepsOverride(null);
  }

  setStatusDepsOverride(
    buildStatusDeps({
      dispatchStatus: null,
      dispatchResult: null,
      executionResult: null,
    })
  );
  try {
    const r4 = await statusGet(new Request("http://localhost"), { params: Promise.resolve({ requestId }) });
    assert.equal(r4.status, 200);
    const b4 = (await readJson(r4)) as { dispatchStatus: DispatchStatus | null; executionStatus: ExecutionStatus | null };
    assert.equal(b4.dispatchStatus, null);
    assert.equal(b4.executionStatus, null);
  } finally {
    setStatusDepsOverride(null);
  }

  const invalid = await statusGet(new Request("http://localhost"), { params: Promise.resolve({ requestId: "0x12" }) });
  assert.equal(invalid.status, 400);
}

async function runHealthApiTests(): Promise<void> {
  setEnv();
  setHealthDepsOverride({
    readDispatchStatus: async () => "Queued",
    readExecutionResult: async () => null,
  });
  try {
    const response = await healthGet();
    assert.equal(response.status, 200);
    const body = (await readJson(response)) as { ok: boolean; evmGatewayReachable: boolean; wrapReachable: boolean };
    assert.equal(body.ok, true);
    assert.equal(body.evmGatewayReachable, true);
    assert.equal(body.wrapReachable, true);
  } finally {
    setHealthDepsOverride(null);
  }
}

async function runWithdrawApiTests(): Promise<void> {
  setEnv();
  const requestId = `0x${"bb".repeat(32)}`;
  setWithdrawDepsOverride({
    withdrawFailedWrap: async () => ({
      requestId: Uint8Array.from(Buffer.from(requestId.slice(2), "hex")),
      ledgerTxId: Uint8Array.from([0xaa, 0xbb]),
    }),
  });
  try {
    const okResponse = await withdrawPost(
      makeRequestJson({
        requestId,
      })
    );
    assert.equal(okResponse.status, 200);
    const okBody = (await readJson(okResponse)) as {
      ok: boolean;
      requestId: string;
      ledgerTxId: string;
    };
    assert.equal(okBody.ok, true);
    assert.equal(okBody.requestId, requestId);
    assert.equal(okBody.ledgerTxId, "0xaabb");

    const badResponse = await withdrawPost(
      makeRequestJson({
        requestId: "0x12",
      })
    );
    assert.equal(badResponse.status, 400);
  } finally {
    setWithdrawDepsOverride(null);
  }
}

async function runConfigFailFastTests(): Promise<void> {
  setEnv();
  delete process.env.WRAP_CANISTER_ID;
  const response = await healthGet();
  assert.equal(response.status, 500);
  const body = (await readJson(response)) as { errorCode: string };
  assert.equal(body.errorCode, "config_missing");
}

async function runSubmitNonceDeterminismTests(): Promise<void> {
  setEnv();
  const makeDeps = {
    readNonce: async () => 0n,
    submitTx: async () => Uint8Array.from([1]),
    readDispatchStatus: async () => "Queued" as const,
  };
  const payload = {
    assetId: "2vxsx-fae",
    amount: "100",
    recipient: "2vxsx-fae",
  };
  const a = await submitUnwrapRequest(payload, { ...makeDeps, makeUserNonce: () => 1n });
  const b = await submitUnwrapRequest(payload, { ...makeDeps, makeUserNonce: () => 2n });
  assert.notEqual(a.requestId, b.requestId);
}

async function main(): Promise<void> {
  await runValidationTests();
  await runRequestIdTests();
  await runMergeTests();
  await runWrapClientExecutionTests();
  await runSubmitApiTests();
  await runStatusApiTests();
  await runHealthApiTests();
  await runWithdrawApiTests();
  await runConfigFailFastTests();
  await runSubmitNonceDeterminismTests();
  process.stdout.write("wrapper tests: ok\n");
}

main().catch((error: unknown) => {
  const message = error instanceof Error ? error.stack ?? error.message : String(error);
  process.stderr.write(`${message}\n`);
  process.exitCode = 1;
});
