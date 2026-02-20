// どこで: Explorerテスト / 何を: hex変換とPostgresクエリを検証 / なぜ: Postgres移行後の退行を防ぐため

import assert from "node:assert/strict";
import { createHmac } from "node:crypto";
import { promises as fs } from "node:fs";
import { newDb } from "pg-mem";
import { NextRequest } from "next/server";
import {
  isAddressHex,
  isTxHashHex,
  normalizeHex,
  parseAddressHex,
  parseHex,
  toHexLower,
} from "../lib/hex";
import {
  closeExplorerPool,
  getBlockDetails,
  getLatestBlocks,
  getLatestTxs,
  getMaxBlockNumber,
  getMetaSnapshot,
  getOpsMetricsSamplesSince,
  getRecentOpsMetricsSamples,
  getTokenTransfersByAddress,
  getOverviewStats,
  getTx,
  getTxsByAddress,
  getTxsByCallerPrincipal,
  claimNextVerifyRequest,
  deleteVerifyReplayExpired,
  getVerifyMetricsSamplesSince,
  getVerifyRequestById,
  insertVerifyRequest,
  markVerifyRequestFailed,
  markVerifyRequestSucceeded,
  requeueVerifyRequest,
  setExplorerPool,
  consumeVerifyReplayJti,
} from "../lib/db";
import { getPrincipalView, opsDataTestHooks, parseCyclesTrendWindow, resolveHomeBlocksLimit } from "../lib/data";
import { buildPruneHistory, parseStoredPruneStatusForTest } from "../lib/data_ops";
import { mapAddressHistory, mapAddressTokenTransfers } from "../lib/data_address";
import { calcRoundedBps, formatEthFromWei, formatGweiFromWei } from "../lib/format";
import { deriveEvmAddressFromPrincipal } from "../lib/principal";
import { logsTestHooks } from "../lib/logs";
import { resolveSearchRoute } from "../lib/search";
import { loadConfig } from "../lib/config";
import {
  canonicalizeVerifyInput,
  compressVerifyPayload,
  decompressVerifyPayload,
  normalizeVerifySubmitInput,
} from "../lib/verify/normalize";
import { authenticateVerifyRequest } from "../lib/verify/auth";
import { isRuntimeMatch } from "../lib/verify/compile";
import { medianBigInt } from "../lib/verify/metrics";
import { executeVerifyJob, isVerifyServiceError } from "../lib/verify/service";
import { buildVerifyAuthToken } from "../lib/verify/token";
import { getTokenMeta } from "../lib/token_meta";
import { createOrGetVerifyRequest } from "../lib/verify/submit";
import { runBackgroundTask, shouldRunPeriodicTask } from "../lib/verify/worker_tasks";
import { parseChainId, parseVerifiedAbi } from "../app/api/contracts/[address]/verified/route";
import { verifyWorkerTestHooks } from "../scripts/verify-worker";
import { tokenMetaTestHooks } from "../lib/token_meta";
import { buildTimelineFromReceiptLogs } from "../lib/tx_timeline";
import { deriveTxDirection } from "../lib/tx_direction";
import { inferMethodLabel } from "../lib/tx_method";
import { txValueFeeCellsTestHooks } from "../components/tx-value-fee-cells";
import type { ReceiptView } from "../lib/rpc";
import type { VerifySubmitInput } from "../lib/verify/types";

async function runHexTests(): Promise<void> {
  const bytes = parseHex("0x00aabb");
  assert.equal(bytes.length, 3);
  assert.equal(toHexLower(bytes), "0x00aabb");
  assert.throws(() => parseHex("0xabc"));
  assert.throws(() => parseHex("0xzz"));
  assert.equal(normalizeHex("AABB"), "0xaabb");
  assert.equal(isAddressHex("0x0000000000000000000000000000000000000000"), true);
  assert.equal(isAddressHex("0x00"), false);
  assert.equal(isTxHashHex("0x" + "11".repeat(32)), true);
  assert.equal(isTxHashHex("0x" + "11".repeat(20)), false);
  assert.equal(parseAddressHex("0x" + "22".repeat(20)).length, 20);
  assert.throws(() => parseAddressHex("0x" + "22".repeat(19)));
}

async function runTxMetricsInputValidationTests(): Promise<void> {
  assert.equal(txValueFeeCellsTestHooks.isValidTxIdHex("0x" + "ab".repeat(32)), true);
  assert.equal(txValueFeeCellsTestHooks.isValidTxIdHex("0x" + "ab".repeat(31)), false);
  assert.equal(txValueFeeCellsTestHooks.isValidTxIdHex("0x" + "ab".repeat(33)), false);
  assert.equal(txValueFeeCellsTestHooks.isValidTxIdHex("ab".repeat(32)), false);
  assert.equal(txValueFeeCellsTestHooks.isValidTxIdHex("0x" + "zz".repeat(32)), false);
}

async function runSearchTests(): Promise<void> {
  assert.equal(resolveSearchRoute("12"), "/blocks/12");
  assert.equal(resolveSearchRoute("0x" + "11".repeat(32)), "/tx/0x" + "11".repeat(32));
  assert.equal(resolveSearchRoute("0x" + "22".repeat(20)), "/address/0x" + "22".repeat(20));
  assert.equal(
    resolveSearchRoute("2vxsx-fae"),
    "/principal/2vxsx-fae"
  );
  assert.equal(resolveSearchRoute("invalid"), "/");
}

async function runHomeBlocksLimitTests(): Promise<void> {
  assert.equal(resolveHomeBlocksLimit(undefined, 10), 10);
  assert.equal(resolveHomeBlocksLimit("20", 10), 20);
  assert.equal(resolveHomeBlocksLimit(["30", "40"], 10), 30);
  assert.equal(resolveHomeBlocksLimit("0", 10), 10);
  assert.equal(resolveHomeBlocksLimit("501", 10), 10);
  assert.equal(resolveHomeBlocksLimit("abc", 10), 10);
}

async function runCyclesTrendWindowTests(): Promise<void> {
  assert.equal(parseCyclesTrendWindow("24h"), "24h");
  assert.equal(parseCyclesTrendWindow("7d"), "7d");
  assert.equal(parseCyclesTrendWindow(undefined), "24h");
  assert.equal(parseCyclesTrendWindow("x"), "24h");
}

async function runPruneHistoryTests(): Promise<void> {
  const out = buildPruneHistory(
    [
      { sampledAtMs: 5000n, prunedBeforeBlock: 12n },
      { sampledAtMs: 4000n, prunedBeforeBlock: 12n },
      { sampledAtMs: 3000n, prunedBeforeBlock: 10n },
      { sampledAtMs: 2000n, prunedBeforeBlock: null },
      { sampledAtMs: 1000n, prunedBeforeBlock: 8n },
    ],
    10
  );
  assert.deepEqual(out, [
    { sampledAtMs: 5000n, prunedBeforeBlock: 12n },
    { sampledAtMs: 3000n, prunedBeforeBlock: 10n },
    { sampledAtMs: 1000n, prunedBeforeBlock: 8n },
  ]);
}

async function runCapacityForecastTests(): Promise<void> {
  const forecast = opsDataTestHooks.buildCapacityForecast(
    [
      { sampledAtMs: 0n, estimatedKeptBytes: 100n * 1024n * 1024n },
      { sampledAtMs: 12n * 60n * 60n * 1000n, estimatedKeptBytes: 110n * 1024n * 1024n },
      { sampledAtMs: 24n * 60n * 60n * 1000n, estimatedKeptBytes: 120n * 1024n * 1024n },
    ],
    24 * 60 * 60 * 1000,
    200n * 1024n * 1024n,
    300n * 1024n * 1024n
  );
  assert.equal(forecast.growthBytesPerDay === null, false);
  if (forecast.growthBytesPerDay === null) {
    throw new Error("growthBytesPerDay should not be null");
  }
  const growthMbPerDay = forecast.growthBytesPerDay / (1024 * 1024);
  assert.equal(Math.round(growthMbPerDay), 20);
  assert.equal(forecast.daysToHighWater === null, false);
  assert.equal(forecast.daysToHardEmergency === null, false);
}

async function runConfigTests(): Promise<void> {
  const cfg = loadConfig({
    ...process.env,
    NODE_ENV: process.env.NODE_ENV ?? "test",
    EXPLORER_DATABASE_URL: "postgres://localhost:5432/test",
    EXPLORER_LATEST_BLOCKS: "500",
  });
  assert.equal(cfg.latestBlocksLimit, 500);
  const fallback = loadConfig({
    ...process.env,
    NODE_ENV: process.env.NODE_ENV ?? "test",
    EXPLORER_DATABASE_URL: "postgres://localhost:5432/test",
    EXPLORER_LATEST_BLOCKS: "900",
  });
  assert.equal(fallback.latestBlocksLimit, 10);
  assert.equal(fallback.verifyEnabled, false);
  assert.equal(fallback.verifyRawPayloadLimitBytes, 5_000_000);
  assert.equal(fallback.verifyWorkerConcurrency, 2);
  assert.equal(fallback.verifyDefaultChainId, 0);
  assert.equal(fallback.verifyRequiredScope, "verify.submit");
  assert.equal(fallback.verifyMetricsRetentionDays, 30);
  const maxInt4 = loadConfig({
    ...process.env,
    NODE_ENV: process.env.NODE_ENV ?? "test",
    EXPLORER_DATABASE_URL: "postgres://localhost:5432/test",
    EXPLORER_VERIFY_DEFAULT_CHAIN_ID: "2147483647",
  });
  assert.equal(maxInt4.verifyDefaultChainId, 2_147_483_647);
  const outOfRange = loadConfig({
    ...process.env,
    NODE_ENV: process.env.NODE_ENV ?? "test",
    EXPLORER_DATABASE_URL: "postgres://localhost:5432/test",
    EXPLORER_VERIFY_DEFAULT_CHAIN_ID: "2147483648",
  });
  assert.equal(outOfRange.verifyDefaultChainId, 0);
}

async function runFormatTests(): Promise<void> {
  assert.equal(formatEthFromWei(30_575_433n), "0.000000000030575433 ICP");
  assert.equal(formatGweiFromWei(30_575_433n), "0.030575433 Gwei");
  assert.equal(formatEthFromWei(1_566_262_114_653_579n), "0.001566262114653579 ICP");
  assert.equal(calcRoundedBps(51_226_163n, 60_000_000n), 8538n);
  assert.equal(calcRoundedBps(8_000_000n, 10_000_000n), 8000n);
  assert.equal(calcRoundedBps(-5n, 2n), -25000n);
  assert.equal(calcRoundedBps(1n, 0n), null);
}

async function runVerifyNormalizeTests(): Promise<void> {
  const input = normalizeVerifySubmitInput({
    chainId: 0,
    contractAddress: "0x" + "11".repeat(20),
    compilerVersion: "0.8.30",
    optimizerEnabled: true,
    optimizerRuns: 200,
    evmVersion: null,
    sourceBundle: {
      "contracts/A.sol": "contract A {}",
    },
    contractName: "A",
    constructorArgsHex: "0x",
  });
  const canonical = canonicalizeVerifyInput(input);
  const restored = decompressVerifyPayload(compressVerifyPayload(canonical));
  assert.equal(restored.contractAddress, input.contractAddress);
  assert.equal(restored.compilerVersion, "0.8.30");
  assert.equal(Object.keys(restored.sourceBundle).length, 1);
  assert.throws(
    () =>
      normalizeVerifySubmitInput({
        ...input,
        sourceBundle: {
          "../A.sol": "contract A {}",
        },
      }),
    /invalid path/
  );
  assert.throws(
    () =>
      normalizeVerifySubmitInput({
        ...input,
        optimizerRuns: 1_000_001,
      }),
    /out of range/
  );
  assert.throws(
    () =>
      normalizeVerifySubmitInput({
        ...input,
        chainId: 2_147_483_648,
      }),
    /out of range/
  );
  assert.throws(
    () =>
      normalizeVerifySubmitInput({
        ...input,
        sourceBundle: {
          "contracts\\A.sol": "contract A {}",
        },
      }),
    /invalid path/
  );
  assert.throws(
    () =>
      normalizeVerifySubmitInput({
        ...input,
        contractName: "A\\B",
      }),
    /forbidden characters/
  );
}

async function runVerifyRuntimeMatchTests(): Promise<void> {
  const op = "6001600055";
  const metadata = "a2646970667358221220";
  const lenBytes = "000a";
  const withMetadata = `${op}${metadata}${lenBytes}`;
  assert.equal(isRuntimeMatch(withMetadata, op), true);
  assert.equal(isRuntimeMatch(op, withMetadata), true);
  assert.equal(isRuntimeMatch("6001600056", op), false);
}

async function runVerifyServiceInvalidInputMapTests(): Promise<void> {
  const invalidInput: VerifySubmitInput = {
    chainId: 0,
    contractAddress: "0x" + "11".repeat(20),
    compilerVersion: "0.8.30",
    optimizerEnabled: true,
    optimizerRuns: 200,
    evmVersion: null,
    sourceBundle: {
      "contracts/A.sol": "contract A {}",
    },
    contractName: "A\\B",
    constructorArgsHex: "0x",
  };
  await assert.rejects(async () => executeVerifyJob(invalidInput), (err: unknown) => {
    if (!isVerifyServiceError(err)) {
      return false;
    }
    return err.code === "invalid_input";
  });
}

async function runVerifyAuthTests(): Promise<void> {
  const { pool } = createVerifyTestPool();
  setExplorerPool(pool);
  const prevEnv = {
    EXPLORER_DATABASE_URL: process.env.EXPLORER_DATABASE_URL,
    EXPLORER_VERIFY_AUTH_HMAC_KEYS: process.env.EXPLORER_VERIFY_AUTH_HMAC_KEYS,
    EXPLORER_VERIFY_REQUIRED_SCOPE: process.env.EXPLORER_VERIFY_REQUIRED_SCOPE,
  };
  try {
    process.env.EXPLORER_DATABASE_URL = "postgres://localhost:5432/test";
    process.env.EXPLORER_VERIFY_AUTH_HMAC_KEYS = "kidA:secretA";
    process.env.EXPLORER_VERIFY_REQUIRED_SCOPE = "verify.submit";
    const token = signVerifyToken({
      kid: "kidA",
      secret: "secretA",
      payload: { sub: "user-1", exp: Math.floor(Date.now() / 1000) + 600, scope: "verify.submit", jti: "jti-1" },
    });
    const request = new NextRequest("http://localhost/api/verify/submit", {
      headers: {
        authorization: `Bearer ${token}`,
      },
    });
    const auth1 = await authenticateVerifyRequest(request);
    assert.equal(auth1?.userId, "user-1");
    const reusedReq = new NextRequest("http://localhost/api/verify/status?id=req-1", {
      headers: { authorization: `Bearer ${token}` },
    });
    const reusedDenied = await authenticateVerifyRequest(reusedReq);
    assert.equal(reusedDenied, null);

    const statusToken = signVerifyToken({
      kid: "kidA",
      secret: "secretA",
      payload: { sub: "user-1", exp: Math.floor(Date.now() / 1000) + 600, scope: "verify.submit", jti: "jti-status" },
    });
    const statusReq = new NextRequest("http://localhost/api/verify/status?id=req-1", {
      headers: { authorization: `Bearer ${statusToken}` },
    });
    const statusAuth1 = await authenticateVerifyRequest(statusReq, { consumeReplay: false });
    const statusAuth2 = await authenticateVerifyRequest(statusReq, { consumeReplay: false });
    assert.equal(statusAuth1?.userId, "user-1");
    assert.equal(statusAuth2?.userId, "user-1");
    const tampered = `${token}x`;
    const tamperedReq = new NextRequest("http://localhost/api/verify/submit", {
      headers: { authorization: `Bearer ${tampered}` },
    });
    const tamperedAuth = await authenticateVerifyRequest(tamperedReq);
    assert.equal(tamperedAuth, null);

    const badScope = signVerifyToken({
      kid: "kidA",
      secret: "secretA",
      payload: { sub: "user-1", exp: Math.floor(Date.now() / 1000) + 600, scope: "verify.read", jti: "jti-2" },
    });
    const badScopeReq = new NextRequest("http://localhost/api/verify/submit", {
      headers: { authorization: `Bearer ${badScope}` },
    });
    const auth3 = await authenticateVerifyRequest(badScopeReq);
    assert.equal(auth3, null);
    const expired = signVerifyToken({
      kid: "kidA",
      secret: "secretA",
      payload: { sub: "user-1", exp: Math.floor(Date.now() / 1000) - 1, scope: "verify.submit", jti: "jti-3" },
    });
    const expiredReq = new NextRequest("http://localhost/api/verify/submit", {
      headers: { authorization: `Bearer ${expired}` },
    });
    const auth4 = await authenticateVerifyRequest(expiredReq);
    assert.equal(auth4, null);
    const replayFirst = await consumeVerifyReplayJti({
      jti: "race-jti-1",
      sub: "user-1",
      scope: "verify.submit",
      expSec: BigInt(Math.floor(Date.now() / 1000) + 600),
      consumedAtMs: 1_000_000n,
    });
    const replaySecond = await consumeVerifyReplayJti({
      jti: "race-jti-1",
      sub: "user-1",
      scope: "verify.submit",
      expSec: BigInt(Math.floor(Date.now() / 1000) + 600),
      consumedAtMs: 1_000_001n,
    });
    assert.equal(replayFirst, true);
    assert.equal(replaySecond, false);

    await pool.query(
      "INSERT INTO verify_auth_replay(jti, sub, scope, exp, consumed_at) VALUES($1, $2, $3, $4, $5), ($6, $7, $8, $9, $10)",
      ["old-jti", "u1", "verify.submit", 1, 1, "new-jti", "u1", "verify.submit", 9999999999, 1]
    );
    const deleted = await deleteVerifyReplayExpired(1000n);
    assert.equal(deleted, 1);
  } finally {
    process.env.EXPLORER_DATABASE_URL = prevEnv.EXPLORER_DATABASE_URL;
    process.env.EXPLORER_VERIFY_AUTH_HMAC_KEYS = prevEnv.EXPLORER_VERIFY_AUTH_HMAC_KEYS;
    process.env.EXPLORER_VERIFY_REQUIRED_SCOPE = prevEnv.EXPLORER_VERIFY_REQUIRED_SCOPE;
    await closeExplorerPool();
  }
}

async function runVerifyRequestLifecycleTests(): Promise<void> {
  const { pool } = createVerifyTestPool();
  setExplorerPool(pool);
  try {
    await insertVerifyRequest({
      id: "req-1",
      contractAddress: "0x" + "11".repeat(20),
      chainId: 0,
      submittedBy: "user-a",
      status: "queued",
      inputHash: "hash-1",
      payloadCompressed: Uint8Array.from([1, 2, 3]),
      createdAtMs: 1000n,
    });
    const claimed = await claimNextVerifyRequest(2000n);
    assert.equal(claimed?.id, "req-1");
    assert.equal(claimed?.status, "running");
    await requeueVerifyRequest({
      id: "req-1",
      errorCode: "rpc_unavailable",
      errorMessage: "retry me",
      updatedAtMs: 2500n,
    });
    const claimedAgain = await claimNextVerifyRequest(3000n);
    assert.equal(claimedAgain?.attempts, 2);
    await markVerifyRequestFailed({
      id: "req-1",
      errorCode: "compile_timeout",
      errorMessage: "timeout",
      finishedAtMs: 3500n,
    });
    const failed = await getVerifyRequestById("req-1");
    assert.equal(failed?.status, "failed");
    await markVerifyRequestSucceeded({
      id: "req-1",
      verifiedContractId: "vc-1",
      finishedAtMs: 3600n,
    });
    const succeeded = await getVerifyRequestById("req-1");
    assert.equal(succeeded?.status, "succeeded");
    assert.equal(succeeded?.verifiedContractId, "vc-1");
  } finally {
    await closeExplorerPool();
  }
}

async function runVerifyMetricsTests(): Promise<void> {
  const { pool } = createVerifyTestPool();
  setExplorerPool(pool);
  try {
    await pool.query(
      "INSERT INTO verify_metrics_samples(sampled_at_ms, queue_depth, success_count, failed_count, avg_duration_ms, p50_duration_ms, p95_duration_ms, fail_by_code_json) VALUES($1, $2, $3, $4, $5, $6, $7, $8), ($9, $10, $11, $12, $13, $14, $15, $16)",
      [
        10_000,
        2,
        1,
        1,
        200,
        180,
        250,
        JSON.stringify({ runtime_mismatch: "1" }),
        20_000,
        1,
        3,
        1,
        300,
        250,
        450,
        JSON.stringify({ compile_timeout: "1" }),
      ]
    );
    await pool.query("DELETE FROM verify_metrics_samples WHERE sampled_at_ms < $1", [15_000]);
    const samples = await getVerifyMetricsSamplesSince(0n);
    assert.equal(samples.length, 1);
    assert.equal(samples[0]?.sampledAtMs, 20_000n);
    const failByCode = JSON.parse(samples[0]?.failByCodeJson ?? "{}") as Record<string, string>;
    assert.equal(failByCode.compile_timeout, "1");
  } finally {
    await closeExplorerPool();
  }
}

async function runVerifyOpsMedianTests(): Promise<void> {
  assert.equal(medianBigInt([]), null);
  assert.equal(medianBigInt([0n]), 0n);
  assert.equal(medianBigInt([5n, 0n, 10n]), 5n);
}

async function runVerifySubmitDuplicateFallbackTests(): Promise<void> {
  const duplicateError = Object.assign(new Error("duplicate key value violates unique constraint"), { code: "23505" });
  let getByHashCount = 0;
  const out = await createOrGetVerifyRequest(
    {
      inputHash: "hash-1",
      id: "new-id",
      contractAddress: "0x" + "11".repeat(20),
      chainId: 0,
      submittedBy: "user-a",
      payloadCompressed: Uint8Array.from([1, 2, 3]),
      createdAtMs: 1000n,
    },
    {
      getByInputHash: async (_submittedBy, _inputHash) => {
        getByHashCount += 1;
        if (getByHashCount === 1) {
          return null;
        }
        return { id: "existing-id", status: "queued" };
      },
      insert: async () => {
        throw duplicateError;
      },
    }
  );
  assert.equal(out.requestId, "existing-id");
  assert.equal(out.status, "queued");
  assert.equal(out.created, false);
  assert.equal(out.httpStatus, 200);
  assert.equal(getByHashCount, 2);
}

async function runVerifySubmitPerUserDedupTests(): Promise<void> {
  const seen = new Map<string, { id: string; status: "queued" }>();
  const makeDeps = () => ({
    getByInputHash: async (submittedBy: string, inputHash: string) => seen.get(`${submittedBy}:${inputHash}`) ?? null,
    insert: async (input: {
      id: string;
      contractAddress: string;
      chainId: number;
      submittedBy: string;
      status: "queued";
      inputHash: string;
      payloadCompressed: Uint8Array;
      createdAtMs: bigint;
    }) => {
      seen.set(`${input.submittedBy}:${input.inputHash}`, { id: input.id, status: "queued" });
    },
  });

  const first = await createOrGetVerifyRequest(
    {
      inputHash: "same-hash",
      id: "user-a-req-1",
      contractAddress: "0x" + "11".repeat(20),
      chainId: 0,
      submittedBy: "user-a",
      payloadCompressed: Uint8Array.from([1]),
      createdAtMs: 1n,
    },
    makeDeps()
  );
  const second = await createOrGetVerifyRequest(
    {
      inputHash: "same-hash",
      id: "user-b-req-1",
      contractAddress: "0x" + "11".repeat(20),
      chainId: 0,
      submittedBy: "user-b",
      payloadCompressed: Uint8Array.from([2]),
      createdAtMs: 2n,
    },
    makeDeps()
  );
  const duplicateSameUser = await createOrGetVerifyRequest(
    {
      inputHash: "same-hash",
      id: "user-a-req-2",
      contractAddress: "0x" + "11".repeat(20),
      chainId: 0,
      submittedBy: "user-a",
      payloadCompressed: Uint8Array.from([3]),
      createdAtMs: 3n,
    },
    makeDeps()
  );

  assert.equal(first.httpStatus, 202);
  assert.equal(second.httpStatus, 202);
  assert.equal(first.requestId, "user-a-req-1");
  assert.equal(second.requestId, "user-b-req-1");
  assert.equal(duplicateSameUser.httpStatus, 200);
  assert.equal(duplicateSameUser.requestId, "user-a-req-1");
}

async function runVerifyWorkerBackgroundTaskTests(): Promise<void> {
  const previousError = console.error;
  const logs: unknown[][] = [];
  console.error = (...args: unknown[]) => {
    logs.push(args);
  };
  try {
    assert.equal(shouldRunPeriodicTask(60_000, 0, 60_000), true);
    assert.equal(shouldRunPeriodicTask(59_999, 0, 60_000), false);
    runBackgroundTask("test_task", async () => {
      throw new Error("boom");
    });
    await sleep(0);
    assert.equal(logs.length, 1);
    assert.equal(String(logs[0]?.[0] ?? "").includes("[verify-worker] test_task failed"), true);
  } finally {
    console.error = previousError;
  }
}

async function runVerifyWorkerTimeoutCleanupTests(): Promise<void> {
  let activeTimers = 0;
  const originalSetTimeout = global.setTimeout;
  const originalClearTimeout = global.clearTimeout;
  global.setTimeout = ((handler: (...args: unknown[]) => void, timeout?: number, ...args: unknown[]) => {
    activeTimers += 1;
    return originalSetTimeout(handler, timeout, ...args);
  }) as typeof global.setTimeout;
  global.clearTimeout = ((timeout: NodeJS.Timeout | number | string | undefined) => {
    if (timeout !== undefined) {
      activeTimers = Math.max(0, activeTimers - 1);
    }
    return originalClearTimeout(timeout);
  }) as typeof global.clearTimeout;
  try {
    const out = await verifyWorkerTestHooks.runWithTimeout(Promise.resolve("ok"), 5000);
    assert.equal(out, "ok");
    assert.equal(activeTimers, 0);
  } finally {
    global.setTimeout = originalSetTimeout;
    global.clearTimeout = originalClearTimeout;
  }
}

async function runVerifyAbiParseFallbackTests(): Promise<void> {
  const ok = parseVerifiedAbi('[{"type":"function","name":"x"}]');
  assert.equal(ok.abiParseError, false);
  assert.equal(Array.isArray(ok.abi), true);
  const bad = parseVerifiedAbi("{not-json");
  assert.equal(bad.abiParseError, true);
  assert.equal(bad.abi, null);
  assert.equal(parseChainId("2147483647", 0), 2_147_483_647);
  assert.equal(parseChainId("2147483648", 0), null);
  assert.equal(parseChainId(null, 2_147_483_648), null);
}

async function runVerifyTokenBuildTests(): Promise<void> {
  const token = buildVerifyAuthToken({
    kid: "kid-a",
    secret: "secret-a",
    sub: "deployer",
    scope: "verify.submit",
    expSec: 1_900_000_000,
    jti: "jti-a",
  });
  const parts = token.split(".");
  assert.equal(parts.length, 3);
  const headerJson = Buffer.from(parts[0] ?? "", "base64url").toString("utf8");
  const payloadJson = Buffer.from(parts[1] ?? "", "base64url").toString("utf8");
  const header = JSON.parse(headerJson) as { alg: string; kid: string; typ: string };
  const payload = JSON.parse(payloadJson) as { sub: string; scope: string; exp: number; jti: string };
  assert.equal(header.alg, "HS256");
  assert.equal(header.kid, "kid-a");
  assert.equal(payload.sub, "deployer");
  assert.equal(payload.scope, "verify.submit");
  assert.equal(payload.exp, 1_900_000_000);
  assert.equal(payload.jti, "jti-a");
}

async function runPrincipalDeriveTests(): Promise<void> {
  const address = deriveEvmAddressFromPrincipal("nggqm-p5ozz-i5hfv-bejmq-2gtow-4dtqw-vjatn-4b4yw-s5mzs-i46su-6ae");
  assert.equal(address, "0xf53e047376e37eac56d48245b725c47410cf6f1e");
}

async function runDependencyPinTests(): Promise<void> {
  const packageJsonRaw = await fs.readFile(new URL("../package.json", import.meta.url), "utf8");
  const packageJson = JSON.parse(packageJsonRaw) as {
    dependencies?: Record<string, string>;
  };
  const pinned = packageJson.dependencies?.["@dfinity/ic-pub-key"];
  assert.equal(pinned, "1.0.1");

  const lockRaw = await fs.readFile(new URL("../package-lock.json", import.meta.url), "utf8");
  const lockJson = JSON.parse(lockRaw) as {
    packages?: Record<string, { version?: string }>;
  };
  const lockVersion = lockJson.packages?.["node_modules/@dfinity/ic-pub-key"]?.version;
  assert.equal(lockVersion, "1.0.1");
}

async function runLogsTests(): Promise<void> {
  const blockHashUnsupported = logsTestHooks.parseFilter({
    fromBlock: "",
    toBlock: "",
    address: "",
    topic0: "",
    blockHash: "0x" + "22".repeat(32),
  });
  assert.equal(blockHashUnsupported.ok, false);
  if (blockHashUnsupported.ok) {
    throw new Error("blockHash unsupported test expected error");
  }
  assert.equal(blockHashUnsupported.error, "blockHash filter is not supported. Use fromBlock/toBlock.");

  assert.equal(
    logsTestHooks.hasAnySearchInput({
      fromBlock: "",
      toBlock: "",
      address: "",
      topic0: "",
      window: "",
    }),
    false
  );
  assert.equal(
    logsTestHooks.hasAnySearchInput({
      fromBlock: "1",
      toBlock: "",
      address: "",
      topic0: "",
      window: "",
    }),
    true
  );
  assert.equal(
    logsTestHooks.hasAnySearchInput(
      {
        fromBlock: "",
        toBlock: "",
        address: "",
        topic0: "",
        window: "",
      },
      "0x" + "22".repeat(32)
    ),
    true
  );

  const defaultWindow = logsTestHooks.parseWindowSize("");
  assert.equal(defaultWindow.ok, true);
  if (!defaultWindow.ok) {
    throw new Error("default window must be valid");
  }
  assert.equal(defaultWindow.window, 20);

  const invalidWindow = logsTestHooks.parseWindowSize("abc");
  assert.equal(invalidWindow.ok, false);
  if (invalidWindow.ok) {
    throw new Error("invalid window test expected error");
  }
  assert.equal(invalidWindow.error, "window must be an integer.");

  assert.deepEqual(logsTestHooks.buildDefaultRange(100n, 20), {
    fromBlock: "81",
    toBlock: "100",
  });
  assert.deepEqual(logsTestHooks.buildDefaultRange(7n, 20), {
    fromBlock: "0",
    toBlock: "7",
  });
}

async function runTokenMetaTests(): Promise<void> {
  tokenMetaTestHooks.resetForTest();
  const dynamic = new Uint8Array([
    ...new Array<number>(31).fill(0), 32, // offset
    ...new Array<number>(31).fill(0), 3, // length
    85, 83, 68, // USD
    ...new Array<number>(29).fill(0),
  ]);
  assert.equal(tokenMetaTestHooks.decodeSymbol(dynamic), "USD");

  const bytes32 = new Uint8Array(32);
  bytes32.set([73, 67, 80]); // ICP
  assert.equal(tokenMetaTestHooks.decodeSymbol(bytes32), "ICP");

  const decimals = new Uint8Array(32);
  decimals[31] = 18;
  assert.equal(tokenMetaTestHooks.decodeDecimals(decimals), 18);

  const originalNow = Date.now();
  let nowMs = originalNow;
  tokenMetaTestHooks.setNowProviderForTest(() => nowMs);
  let fetchCount = 0;
  tokenMetaTestHooks.setFetcherForTest(async () => {
    fetchCount += 1;
    return { symbol: "AAA", decimals: 18 };
  });

  const addrA = "0x" + "01".repeat(20);
  const first = await getTokenMeta(addrA);
  const second = await getTokenMeta(addrA);
  assert.equal(first.symbol, "AAA");
  assert.equal(second.symbol, "AAA");
  assert.equal(fetchCount, 1);

  nowMs += tokenMetaTestHooks.constants.SUCCESS_TTL_MS + 1;
  await getTokenMeta(addrA);
  assert.equal(fetchCount, 2);

  tokenMetaTestHooks.resetForTest();
  nowMs = originalNow;
  tokenMetaTestHooks.setNowProviderForTest(() => nowMs);
  let fail = true;
  fetchCount = 0;
  tokenMetaTestHooks.setFetcherForTest(async () => {
    fetchCount += 1;
    if (fail) {
      throw new Error("rpc failed");
    }
    return { symbol: "BBB", decimals: 6 };
  });
  const failed = await getTokenMeta(addrA);
  assert.equal(failed.symbol, null);
  assert.equal(tokenMetaTestHooks.getIsErrorForTest(addrA), true);
  await getTokenMeta(addrA);
  assert.equal(fetchCount, 1);
  fail = false;
  nowMs += tokenMetaTestHooks.constants.ERROR_TTL_MS + 1;
  const recovered = await getTokenMeta(addrA);
  assert.equal(recovered.symbol, "BBB");
  assert.equal(fetchCount, 2);

  tokenMetaTestHooks.resetForTest();
  tokenMetaTestHooks.setNowProviderForTest(() => nowMs);
  fetchCount = 0;
  tokenMetaTestHooks.setFetcherForTest(async () => {
    fetchCount += 1;
    return { symbol: "LRU", decimals: 18 };
  });
  const maxEntries = tokenMetaTestHooks.constants.MAX_CACHE_ENTRIES;
  for (let i = 0; i < maxEntries; i += 1) {
    await getTokenMeta(addressFromIndex(i));
  }
  assert.equal(tokenMetaTestHooks.getCacheSizeForTest(), maxEntries);
  await getTokenMeta(addressFromIndex(maxEntries));
  assert.equal(tokenMetaTestHooks.getCacheSizeForTest(), maxEntries);
  const beforeEvictedFetch = fetchCount;
  await getTokenMeta(addressFromIndex(0));
  assert.equal(fetchCount, beforeEvictedFetch + 1);

  tokenMetaTestHooks.resetForTest();
  tokenMetaTestHooks.setNowProviderForTest(() => nowMs);
  let inflight = 0;
  let maxInflight = 0;
  tokenMetaTestHooks.setFetcherForTest(async () => {
    inflight += 1;
    if (inflight > maxInflight) {
      maxInflight = inflight;
    }
    await sleep(20);
    inflight -= 1;
    return { symbol: "C", decimals: 18 };
  });
  await Promise.all(Array.from({ length: 20 }, (_, i) => getTokenMeta(addressFromIndex(10_000 + i))));
  assert.equal(maxInflight <= tokenMetaTestHooks.constants.MAX_CONCURRENT_FETCHES, true);
  tokenMetaTestHooks.resetForTest();
}

async function runTimelineTests(): Promise<void> {
  const aavePool = bytes("11".repeat(20));
  const initiator = bytes("22".repeat(20));
  const receiver = bytes("33".repeat(20));
  const asset = bytes("44".repeat(20));
  const user = bytes("55".repeat(20));
  const pair = bytes("66".repeat(20));
  const token = bytes("77".repeat(20));

  const timeline = buildTimelineFromReceiptLogs(
    buildReceipt([
      {
        address: aavePool,
        topics: [
          bytes("631042c832b07452973831137f2d73e395028b44b250dedc5abb0ee766e168ac"),
          toTopic(receiver),
          toTopic(initiator),
          toTopic(asset),
        ],
        data: concatWords([3n, 1n, 0n]),
      },
      {
        address: pair,
        topics: [
          bytes("d78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822"),
          toTopic(user),
          toTopic(user),
        ],
        data: concatWords([10n, 0n, 0n, 9n]),
      },
      {
        address: asset,
        topics: [
          bytes("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"),
          toTopic(user),
          toTopic(aavePool),
        ],
        data: concatWords([4n]),
      },
      {
        address: token,
        topics: [bytes("ddf252ad")],
        data: bytes("00"),
      },
    ])
  );

  assert.equal(timeline.steps.length, 4);
  assert.equal(timeline.steps[0]?.type, "flash_borrow");
  assert.equal(timeline.steps[0]?.protocol, "aave");
  assert.equal(timeline.steps[1]?.type, "swap");
  assert.equal(timeline.steps[1]?.protocol, "uniswap_v2");
  assert.equal(timeline.steps[2]?.type, "repay_candidate");
  assert.equal(timeline.steps[2]?.protocol, "aave");
  assert.equal(timeline.steps[3]?.type, "unknown");
  assert.equal(timeline.counters.borrow, 1);
  assert.equal(timeline.counters.swap, 1);
  assert.equal(timeline.counters.repay, 1);
  assert.equal(timeline.counters.unknown, 1);
  assert.equal(timeline.steps[0]?.index, 0);
  assert.equal(timeline.steps[1]?.index, 1);
  assert.equal(timeline.steps[2]?.index, 2);
  assert.equal(timeline.steps[3]?.index, 3);

  const transferApprovalTimeline = buildTimelineFromReceiptLogs(
    buildReceipt([
      {
        address: token,
        topics: [
          bytes("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"),
          toTopic(user),
          toTopic(receiver),
        ],
        data: concatWords([7n]),
      },
      {
        address: token,
        topics: [
          bytes("8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925"),
          toTopic(user),
          toTopic(receiver),
        ],
        data: concatWords([8n]),
      },
    ])
  );
  assert.equal(transferApprovalTimeline.steps[0]?.type, "transfer");
  assert.equal(transferApprovalTimeline.steps[1]?.type, "approval");

  const reverseOrderTimeline = buildTimelineFromReceiptLogs(
    buildReceipt([
      {
        address: token,
        topics: [
          bytes("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"),
          toTopic(user),
          toTopic(aavePool),
        ],
        data: concatWords([5n]),
      },
      {
        address: aavePool,
        topics: [
          bytes("631042c832b07452973831137f2d73e395028b44b250dedc5abb0ee766e168ac"),
          toTopic(receiver),
          toTopic(initiator),
          toTopic(asset),
        ],
        data: concatWords([5n, 1n, 0n]),
      },
    ])
  );
  assert.equal(reverseOrderTimeline.steps[0]?.type, "transfer");
  assert.equal(reverseOrderTimeline.steps[0]?.protocol, "erc20");

  const aaveV3Pool = bytes("88".repeat(20));
  const v3Initiator = bytes("99".repeat(20));
  const v3Asset = bytes("aa".repeat(20));
  const aaveV3Timeline = buildTimelineFromReceiptLogs(
    buildReceipt([
      {
        address: aaveV3Pool,
        topics: [
          bytes("efefaba5e921573100900a3ad9cf29f222d995fb3b6045797eaea7521bd8d6f0"),
          toTopic(receiver),
          toTopic(v3Initiator),
        ],
        data: concatWords([addressWord(v3Asset), 9n, 0n, 2n, 0n]),
      },
      {
        address: aaveV3Pool,
        topics: [
          bytes("f164a7d9b7e450d8229718aed20376118864bcc756709e0fc1d0891133dd2fe8"),
          toTopic(receiver),
          toTopic(v3Initiator),
          toTopic(v3Asset),
        ],
        data: concatWords([11n, 3n, 0n]),
      },
    ])
  );
  const v3Summary = aaveV3Timeline.steps[0]?.summary ?? "";
  assert.equal(v3Summary.includes("asset=0x" + "aa".repeat(20)), true);
  assert.equal(v3Summary.includes("initiator=0x" + "99".repeat(20)), true);
  const simpleSummary = aaveV3Timeline.steps[1]?.summary ?? "";
  assert.equal(simpleSummary.includes("asset=0x" + "aa".repeat(20)), true);
  assert.equal(simpleSummary.includes("initiator=0x" + "99".repeat(20)), true);
}

async function runDbTests(): Promise<void> {
  const mem = newDb({ noAstCoverageCheck: true });
  mem.public.none(`
    CREATE TABLE blocks(number bigint primary key, hash bytea, timestamp bigint not null, tx_count integer not null, gas_used bigint);
    CREATE TABLE txs(tx_hash bytea primary key, block_number bigint not null, tx_index integer not null, caller_principal bytea, from_address bytea not null, to_address bytea, tx_selector bytea, receipt_status smallint);
    CREATE TABLE tx_receipts_index(tx_hash bytea primary key, contract_address bytea, status smallint not null, block_number bigint not null, tx_index integer not null);
    CREATE TABLE token_transfers(tx_hash bytea not null, block_number bigint not null, tx_index integer not null, log_index integer not null, token_address bytea not null, from_address bytea not null, to_address bytea not null, amount_numeric numeric(78,0) not null, primary key(tx_hash, log_index));
    CREATE TABLE metrics_daily(day integer primary key, raw_bytes bigint not null default 0, compressed_bytes bigint not null default 0, archive_bytes bigint, blocks_ingested bigint not null default 0, errors bigint not null default 0);
    CREATE TABLE ops_metrics_samples(sampled_at_ms bigint primary key, queue_len bigint not null, cycles bigint not null default 0, pruned_before_block bigint, estimated_kept_bytes bigint, low_water_bytes bigint, high_water_bytes bigint, hard_emergency_bytes bigint, total_submitted bigint not null, total_included bigint not null, total_dropped bigint not null, drop_counts_json text not null);
    CREATE TABLE meta(key text primary key, value text);
  `);

  const adapter = mem.adapters.createPg();
  const pool = new adapter.Pool();
  setExplorerPool(pool);
  await pool.query("INSERT INTO blocks(number, hash, timestamp, tx_count, gas_used) VALUES($1, $2, $3, $4, $5)", [12, Buffer.from("aa", "hex"), 1000, 1, 21000]);
  await pool.query("INSERT INTO blocks(number, hash, timestamp, tx_count, gas_used) VALUES($1, $2, $3, $4, $5)", [11, Buffer.from("bb", "hex"), 900, 1, 20000]);
  await pool.query("INSERT INTO txs(tx_hash, block_number, tx_index, caller_principal, from_address, to_address, tx_selector, receipt_status) VALUES($1, $2, $3, $4, $5, $6, $7, $8)", [
    Buffer.from("1122", "hex"),
    12,
    0,
    null,
    Buffer.from("11".repeat(20), "hex"),
    Buffer.from("22".repeat(20), "hex"),
    Buffer.from("01020304", "hex"),
    1,
  ]);
  await pool.query("INSERT INTO txs(tx_hash, block_number, tx_index, caller_principal, from_address, to_address, tx_selector, receipt_status) VALUES($1, $2, $3, $4, $5, $6, $7, $8)", [
    Buffer.from("3344", "hex"),
    11,
    0,
    Buffer.from([4]),
    Buffer.from("22".repeat(20), "hex"),
    Buffer.from("11".repeat(20), "hex"),
    Buffer.from("095ea7b3", "hex"),
    0,
  ]);
  await pool.query("INSERT INTO txs(tx_hash, block_number, tx_index, caller_principal, from_address, to_address, tx_selector, receipt_status) VALUES($1, $2, $3, $4, $5, $6, $7, $8)", [
    Buffer.from("5566", "hex"),
    10,
    1,
    null,
    Buffer.from("11".repeat(20), "hex"),
    Buffer.from("11".repeat(20), "hex"),
    null,
    null,
  ]);
  await pool.query("INSERT INTO token_transfers(tx_hash, block_number, tx_index, log_index, token_address, from_address, to_address, amount_numeric) VALUES($1, $2, $3, $4, $5, $6, $7, $8)", [
    Buffer.from("1122", "hex"),
    12,
    0,
    0,
    Buffer.from("99".repeat(20), "hex"),
    Buffer.from("22".repeat(20), "hex"),
    Buffer.from("11".repeat(20), "hex"),
    "250000000000000000",
  ]);
  await pool.query("INSERT INTO tx_receipts_index(tx_hash, contract_address, status, block_number, tx_index) VALUES($1, $2, $3, $4, $5)", [
    Buffer.from("3344", "hex"),
    Buffer.from("33".repeat(20), "hex"),
    0,
    11,
    0,
  ]);
  await pool.query(
    "INSERT INTO ops_metrics_samples(sampled_at_ms, queue_len, cycles, pruned_before_block, estimated_kept_bytes, low_water_bytes, high_water_bytes, hard_emergency_bytes, total_submitted, total_included, total_dropped, drop_counts_json) VALUES($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12), ($13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24)",
    [1000, 1, 200, 9, 900, 800, 1000, 1200, 10, 5, 2, "[]", 2000, 2, 199, 10, 950, 800, 1000, 1200, 12, 5, 3, '[{\"code\":1,\"count\":3}]']
  );
  await pool.query(
    "INSERT INTO metrics_daily(day, raw_bytes, compressed_bytes, archive_bytes, blocks_ingested, errors) VALUES($1, $2, $3, $4, $5, $6)",
    [20260215, 99, 55, 55, 2, 0]
  );
  await pool.query(
    "INSERT INTO meta(key, value) VALUES($1, $2), ($3, $4), ($5, $6), ($7, $8)",
    [
      "need_prune",
      "1",
      "last_head",
      "12",
      "last_ingest_at",
      "1700000000000",
      "prune_status",
      JSON.stringify({ v: 1, fetched_at_ms: "1700000000000", status: { need_prune: true } }),
    ]
  );

  const head = await getMaxBlockNumber();
  assert.equal(head, 12n);
  const latestBlock = (await getLatestBlocks(1))[0];
  const latestTx = (await getLatestTxs(1))[0];
  assert.ok(latestBlock);
  assert.ok(latestTx);
  assert.equal(latestBlock?.number, 12n);
  assert.equal(latestBlock?.gasUsed, 21000n);
  assert.equal(latestTx?.txHashHex, "0x1122");
  assert.equal(latestTx?.createdContractAddress, null);
  assert.equal((await getBlockDetails(12n))?.txs.length, 1);
  assert.equal((await getTx(Uint8Array.from(Buffer.from("1122", "hex"))))?.blockNumber, 12n);
  assert.equal((await getTx(Uint8Array.from(Buffer.from("1122", "hex"))))?.receiptStatus, 1);
  assert.equal((await getTx(Uint8Array.from(Buffer.from("3344", "hex"))))?.createdContractAddress?.toString("hex"), "33".repeat(20));
  const byPrincipal = await getTxsByCallerPrincipal(Uint8Array.from([4]), 10);
  assert.equal(byPrincipal.length, 1);
  assert.equal(byPrincipal[0]?.txHashHex, "0x3344");
  const prevDatabaseUrl = process.env.EXPLORER_DATABASE_URL;
  const prevPrincipalTxs = process.env.EXPLORER_PRINCIPAL_TXS;
  try {
    process.env.EXPLORER_DATABASE_URL = "postgres://test";
    process.env.EXPLORER_PRINCIPAL_TXS = "1";
    const principal = await getPrincipalView("2vxsx-fae");
    assert.equal(principal.principalText, "2vxsx-fae");
    assert.equal(principal.txs.length, 1);
  } finally {
    process.env.EXPLORER_DATABASE_URL = prevDatabaseUrl;
    process.env.EXPLORER_PRINCIPAL_TXS = prevPrincipalTxs;
  }
  assert.equal((await getOverviewStats()).latestDay, 20260215);
  const meta = await getMetaSnapshot();
  assert.equal(meta.needPrune, true);
  assert.equal(meta.lastHead, 12n);
  assert.equal(meta.lastIngestAtMs, 1700000000000n);
  assert.ok(meta.pruneStatusRaw);
  assert.equal(meta.memoryBreakdownRaw, null);
  const address = Uint8Array.from(Buffer.from("11".repeat(20), "hex"));
  const txsByAddress = await getTxsByAddress(address, 2, null);
  assert.equal(txsByAddress.length, 3);
  assert.equal(txsByAddress[0]?.txHashHex, "0x1122");
  assert.equal(txsByAddress[1]?.txHashHex, "0x3344");
  assert.equal(txsByAddress[0]?.blockTimestamp, 1000n);
  assert.equal(txsByAddress[0]?.txSelector?.toString("hex"), "01020304");
  const next = txsByAddress[1];
  assert.ok(next);
  const page2 = await getTxsByAddress(address, 2, {
    blockNumber: next!.blockNumber,
    txIndex: next!.txIndex,
    txHash: Uint8Array.from(Buffer.from(next!.txHashHex.slice(2), "hex")),
  });
  assert.equal(page2.length, 1);
  assert.equal(page2[0]?.txHashHex, "0x5566");
  const tokenTransfers = await getTokenTransfersByAddress(address, 10, null);
  assert.equal(tokenTransfers.length, 1);
  assert.equal(tokenTransfers[0]?.txHashHex, "0x1122");
  assert.equal(tokenTransfers[0]?.blockTimestamp, 1000n);
  assert.equal(tokenTransfers[0]?.txSelector?.toString("hex"), "01020304");
  assert.equal(tokenTransfers[0]?.amount, 250000000000000000n);
  const opsSamples = await getRecentOpsMetricsSamples(10);
  assert.equal(opsSamples.length, 2);
  assert.equal(opsSamples[0]?.sampledAtMs, 2000n);
  assert.equal(opsSamples[0]?.prunedBeforeBlock, 10n);
  assert.equal(opsSamples[0]?.estimatedKeptBytes, 950n);
  const opsSamples24h = await getOpsMetricsSamplesSince(1500n);
  assert.equal(opsSamples24h.length, 1);
  assert.equal(opsSamples24h[0]?.sampledAtMs, 2000n);

  await closeExplorerPool();
}

async function runDataTests(): Promise<void> {
  const parsed = parseStoredPruneStatusForTest(
    JSON.stringify({
      fetched_at_ms: "1700000000000",
      status: {
        pruning_enabled: true,
        prune_running: false,
        need_prune: true,
        pruned_before_block: "100",
        oldest_kept_block: "101",
        oldest_kept_timestamp: "1700000000001",
        estimated_kept_bytes: "200",
        high_water_bytes: "300",
        low_water_bytes: "250",
        hard_emergency_bytes: "400",
        last_prune_at: "1700000000002",
      },
    })
  );
  assert.ok(parsed?.status);
  assert.equal(parsed?.status?.pruningEnabled, true);
  assert.equal(parsed?.status?.highWaterBytes, 300n);

  const invalid = parseStoredPruneStatusForTest(
    JSON.stringify({
      fetched_at_ms: "1700000000000",
      status: {
        pruning_enabled: "invalid",
        prune_running: false,
        need_prune: true,
        pruned_before_block: "100",
        oldest_kept_block: "101",
        oldest_kept_timestamp: "1700000000001",
        estimated_kept_bytes: "200",
        high_water_bytes: "300",
        low_water_bytes: "250",
        hard_emergency_bytes: "400",
        last_prune_at: "1700000000002",
      },
    })
  );
  assert.equal(invalid?.status, null);
  assert.equal(parseStoredPruneStatusForTest("{"), null);
}

async function runAddressHistoryMappingTests(): Promise<void> {
  const target = "0x" + "11".repeat(20);
  const inRow = mapAddressHistory(
    [
      {
        txHashHex: "0x" + "aa".repeat(32),
        blockNumber: 100n,
        blockTimestamp: 999n,
        txIndex: 0,
        callerPrincipal: null,
        fromAddress: Buffer.from("22".repeat(20), "hex"),
        toAddress: Buffer.from("11".repeat(20), "hex"),
        createdContractAddress: null,
        txSelector: Buffer.from("a9059cbb", "hex"),
        receiptStatus: 1,
      },
    ],
    target
  );
  assert.equal(inRow.length, 1);
  assert.equal(inRow[0]?.direction, "in");
  assert.equal(inRow[0]?.fromAddressHex, "0x" + "22".repeat(20));
  assert.equal(inRow[0]?.toAddressHex, "0x" + "11".repeat(20));
  assert.equal(inRow[0]?.methodLabel, "transfer");

  const createRow = mapAddressHistory(
    [
      {
        txHashHex: "0x" + "bb".repeat(32),
        blockNumber: 101n,
        blockTimestamp: 1000n,
        txIndex: 1,
        callerPrincipal: null,
        fromAddress: Buffer.from("11".repeat(20), "hex"),
        toAddress: null,
        createdContractAddress: Buffer.from("33".repeat(20), "hex"),
        txSelector: null,
        receiptStatus: 0,
      },
    ],
    target
  );
  assert.equal(createRow.length, 1);
  assert.equal(createRow[0]?.direction, "out");
  assert.equal(createRow[0]?.fromAddressHex, "0x" + "11".repeat(20));
  assert.equal(createRow[0]?.toAddressHex, null);
  assert.equal(createRow[0]?.createdContractAddressHex, "0x" + "33".repeat(20));
  assert.equal(createRow[0]?.methodLabel, "create");
}

async function runTxMethodTests(): Promise<void> {
  assert.equal(inferMethodLabel(null, null), "create");
  assert.equal(inferMethodLabel("0x" + "11".repeat(20), null), "call");
  assert.equal(inferMethodLabel("0x" + "11".repeat(20), Buffer.from("a9059cbb", "hex")), "transfer");
  assert.equal(inferMethodLabel("0x" + "11".repeat(20), Buffer.from("60e06040", "hex")), "0x60e06040");
}

async function runTxDirectionTests(): Promise<void> {
  const from = Buffer.from("11".repeat(20), "hex");
  assert.equal(deriveTxDirection(from, Buffer.from("22".repeat(20), "hex")), "out");
  assert.equal(deriveTxDirection(from, Buffer.from("11".repeat(20), "hex")), "self");
  assert.equal(deriveTxDirection(from, null), "out");
}

async function runAddressTokenTransferMappingTests(): Promise<void> {
  const target = "0x" + "11".repeat(20);
  const mapped = mapAddressTokenTransfers(
    [
      {
        txHashHex: "0x" + "cc".repeat(32),
        blockNumber: 120n,
        blockTimestamp: 1100n,
        txIndex: 0,
        logIndex: 2,
        txSelector: Buffer.from("095ea7b3", "hex"),
        tokenAddress: Buffer.from("99".repeat(20), "hex"),
        fromAddress: Buffer.from("11".repeat(20), "hex"),
        toAddress: Buffer.from("22".repeat(20), "hex"),
        amount: 123n,
      },
    ],
    target
  );
  assert.equal(mapped.length, 1);
  assert.equal(mapped[0]?.direction, "out");
  assert.equal(mapped[0]?.blockTimestamp, 1100n);
  assert.equal(mapped[0]?.txSelectorHex, "0x095ea7b3");
  assert.equal(mapped[0]?.methodLabel, "approve");
}

runHexTests()
  .then(runTxMetricsInputValidationTests)
  .then(runSearchTests)
  .then(runHomeBlocksLimitTests)
  .then(runCyclesTrendWindowTests)
  .then(runPruneHistoryTests)
  .then(runCapacityForecastTests)
  .then(runConfigTests)
  .then(runFormatTests)
  .then(runVerifyNormalizeTests)
  .then(runVerifyRuntimeMatchTests)
  .then(runVerifyServiceInvalidInputMapTests)
  .then(runVerifyAuthTests)
  .then(runVerifyRequestLifecycleTests)
  .then(runVerifyMetricsTests)
  .then(runVerifyOpsMedianTests)
  .then(runVerifySubmitDuplicateFallbackTests)
  .then(runVerifySubmitPerUserDedupTests)
  .then(runVerifyWorkerBackgroundTaskTests)
  .then(runVerifyWorkerTimeoutCleanupTests)
  .then(runVerifyAbiParseFallbackTests)
  .then(runVerifyTokenBuildTests)
  .then(runPrincipalDeriveTests)
  .then(runDependencyPinTests)
  .then(runLogsTests)
  .then(runTokenMetaTests)
  .then(runTimelineTests)
  .then(runDbTests)
  .then(runDataTests)
  .then(runAddressHistoryMappingTests)
  .then(runTxMethodTests)
  .then(runTxDirectionTests)
  .then(runAddressTokenTransferMappingTests)
  .then(() => {
    console.log("ok");
  })
  .catch((err) => {
    console.error(err);
    process.exit(1);
  });

function createVerifyTestPool() {
  const mem = newDb({ noAstCoverageCheck: true });
  mem.public.none(`
    CREATE TABLE verify_auth_replay(jti text primary key, sub text not null, scope text not null, exp bigint not null, consumed_at bigint not null);
    CREATE TABLE verify_requests(
      id text primary key,
      contract_address text not null,
      chain_id integer not null,
      submitted_by text not null,
      status text not null,
      input_hash text not null,
      payload_compressed bytea not null,
      error_code text,
      error_message text,
      started_at bigint,
      finished_at bigint,
      attempts integer not null default 0,
      verified_contract_id text,
      created_at bigint not null,
      updated_at bigint not null
    );
    CREATE UNIQUE INDEX uq_verify_requests_submitted_input_hash ON verify_requests(submitted_by, input_hash);
    CREATE TABLE verify_metrics_samples(
      sampled_at_ms bigint primary key,
      queue_depth bigint not null,
      success_count bigint not null,
      failed_count bigint not null,
      avg_duration_ms bigint,
      p50_duration_ms bigint,
      p95_duration_ms bigint,
      fail_by_code_json text not null
    );
  `);
  const adapter = mem.adapters.createPg();
  return { pool: new adapter.Pool() };
}

function signVerifyToken(input: {
  kid: string;
  secret: string;
  payload: { sub: string; exp: number; scope: string; jti: string };
}): string {
  const header = base64UrlEncode(JSON.stringify({ alg: "HS256", kid: input.kid, typ: "JWT" }));
  const payload = base64UrlEncode(JSON.stringify(input.payload));
  const message = `${header}.${payload}`;
  const sig = createHmac("sha256", input.secret).update(message).digest("base64url");
  return `${message}.${sig}`;
}

function base64UrlEncode(text: string): string {
  return Buffer.from(text, "utf8").toString("base64url");
}

function buildReceipt(logs: ReceiptView["logs"]): ReceiptView {
  return {
    tx_id: bytes("00".repeat(32)),
    block_number: 1n,
    tx_index: 0,
    status: 1,
    gas_used: 0n,
    effective_gas_price: 0n,
    l1_data_fee: 0n,
    operator_fee: 0n,
    total_fee: 0n,
    contract_address: [],
    return_data_hash: bytes("00".repeat(32)),
    return_data: [],
    logs,
  };
}

function bytes(hexNoPrefix: string): Uint8Array {
  return Uint8Array.from(Buffer.from(hexNoPrefix, "hex"));
}

function word(value: bigint): Uint8Array {
  const out = new Uint8Array(32);
  let current = value;
  for (let i = 31; i >= 0; i -= 1) {
    out[i] = Number(current & 0xffn);
    current >>= 8n;
  }
  return out;
}

function toTopic(address: Uint8Array): Uint8Array {
  const out = new Uint8Array(32);
  out.set(address, 12);
  return out;
}

function concatWords(values: bigint[]): Uint8Array {
  const out = new Uint8Array(values.length * 32);
  for (let i = 0; i < values.length; i += 1) {
    const current = values[i];
    out.set(word(current === undefined ? 0n : current), i * 32);
  }
  return out;
}

function addressWord(address: Uint8Array): bigint {
  if (address.length !== 20) {
    throw new Error("address must be 20 bytes");
  }
  let out = 0n;
  for (const value of address) {
    out = (out << 8n) | BigInt(value);
  }
  return out;
}

function addressFromIndex(index: number): string {
  const hex = index.toString(16).padStart(40, "0");
  return "0x" + hex.slice(-40);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}
