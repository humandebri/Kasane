// どこで: Explorer verifyワーカー / 何を: verifyキューを非同期処理して結果をDB反映 / なぜ: indexer同期処理と負荷を分離するため

import { createHash, randomUUID } from "node:crypto";
import { pathToFileURL } from "node:url";
import { gzipSync } from "node:zlib";
import {
  addVerifyMetricsSample,
  appendVerifyJobLog,
  claimNextVerifyRequest,
  closeExplorerPool,
  deleteVerifyReplayExpired,
  deleteVerifyLogsOlderThan,
  markVerifyRequestFailed,
  markVerifyRequestSucceeded,
  requeueVerifyRequest,
  upsertVerifiedContract,
  upsertVerifyBlob,
} from "../lib/db";
import { type ExplorerConfig, loadConfig } from "../lib/config";
import { ensureSolcBinaryAvailable } from "../lib/verify/compile";
import { executeVerifyJob, isVerifyServiceError } from "../lib/verify/service";
import { decompressVerifyPayload } from "../lib/verify/normalize";
import { encodeSourceBundle } from "../lib/verify/source_bundle";
import { runBackgroundTask, shouldRunPeriodicTask } from "../lib/verify/worker_tasks";

let stopRequested = false;
const PERIODIC_TASK_INTERVAL_MS = 60_000;

async function main(): Promise<void> {
  const cfg = loadConfig(process.env);
  if (!cfg.verifyEnabled) {
    console.error("[verify-worker] verify is disabled");
    return;
  }
  await verifyCompilerPreflight(cfg);
  console.log(`[verify-worker] starting concurrency=${cfg.verifyWorkerConcurrency}`);
  process.on("SIGINT", () => {
    stopRequested = true;
  });
  process.on("SIGTERM", () => {
    stopRequested = true;
  });
  let lastLogPurgeMs = 0;
  let lastMetricsSampleMs = 0;
  let lastReplayPurgeMs = 0;
  const workers = Array.from({ length: cfg.verifyWorkerConcurrency }, (_, i) =>
    runLoop(i + 1, cfg, () => {
      const nowMs = Date.now();
      if (shouldRunPeriodicTask(nowMs, lastLogPurgeMs, PERIODIC_TASK_INTERVAL_MS)) {
        lastLogPurgeMs = nowMs;
        runBackgroundTask("purge_verify_logs", async () => {
          await purgeLogsIfNeeded(cfg.verifyLogRetentionDays);
        });
      }
      if (shouldRunPeriodicTask(nowMs, lastMetricsSampleMs, cfg.verifyMetricsSampleIntervalMs)) {
        lastMetricsSampleMs = nowMs;
        runBackgroundTask("sample_verify_metrics", async () => {
          await addVerifyMetricsSample({
            sampledAtMs: BigInt(nowMs),
            windowMs: BigInt(cfg.verifyMetricsSampleIntervalMs),
            retentionCutoffMs: BigInt(nowMs - cfg.verifyMetricsRetentionDays * 24 * 60 * 60 * 1000),
          });
        });
      }
      if (shouldRunPeriodicTask(nowMs, lastReplayPurgeMs, PERIODIC_TASK_INTERVAL_MS)) {
        lastReplayPurgeMs = nowMs;
        runBackgroundTask("purge_verify_replay", async () => {
          await deleteVerifyReplayExpired(BigInt(Math.floor(nowMs / 1000)));
        });
      }
    })
  );
  await Promise.all(workers);
  await closeExplorerPool();
}

async function verifyCompilerPreflight(cfg: ExplorerConfig): Promise<void> {
  if (cfg.verifyAllowedCompilerVersions.length === 0) {
    throw new Error("verify worker requires EXPLORER_VERIFY_ALLOWED_COMPILER_VERSIONS");
  }
  for (const version of cfg.verifyAllowedCompilerVersions) {
    await ensureSolcBinaryAvailable(version);
  }
}

async function runLoop(workerId: number, cfg: ExplorerConfig, onLoop: () => void): Promise<void> {
  while (!stopRequested) {
    onLoop();
    const nowMs = BigInt(Date.now());
    const request = await claimNextVerifyRequest(nowMs);
    if (!request) {
      await sleep(1000);
      continue;
    }
    await appendVerifyJobLog({
      id: randomUUID(),
      requestId: request.id,
      level: "info",
      message: `worker_${workerId}: started attempt=${request.attempts}`,
      createdAtMs: nowMs,
      submittedBy: request.submittedBy,
      eventType: "start",
    });
    try {
      const input = decompressVerifyPayload(request.payloadCompressed);
      const result = await runWithTimeout(executeVerifyJob(input), cfg.verifyJobTimeoutMs);
      const source = encodeSourceBundle(input.sourceBundle);
      const metadataRaw = Buffer.from(result.metadataJson, "utf8");
      const metadataGzip = gzipSync(metadataRaw);
      const sourceBlobId = await upsertVerifyBlob({
        id: randomUUID(),
        sha256: sha256(source.raw),
        encoding: "gzip",
        rawSize: source.raw.byteLength,
        blob: source.gzip,
      });
      const metadataBlobId = await upsertVerifyBlob({
        id: randomUUID(),
        sha256: sha256(metadataRaw),
        encoding: "gzip",
        rawSize: metadataRaw.byteLength,
        blob: metadataGzip,
      });
      const verifiedContractId = await upsertVerifiedContract({
        id: randomUUID(),
        contractAddress: input.contractAddress,
        chainId: input.chainId,
        contractName: input.contractName,
        compilerVersion: input.compilerVersion,
        optimizerEnabled: input.optimizerEnabled,
        optimizerRuns: input.optimizerRuns,
        evmVersion: input.evmVersion,
        creationMatch: result.creationMatch,
        runtimeMatch: result.runtimeMatch,
        abiJson: result.abiJson,
        sourceBlobId,
        metadataBlobId,
        publishedAtMs: BigInt(Date.now()),
      });
      await markVerifyRequestSucceeded({
        id: request.id,
        verifiedContractId,
        finishedAtMs: BigInt(Date.now()),
      });
      await appendVerifyJobLog({
        id: randomUUID(),
        requestId: request.id,
        level: "info",
        message: `worker_${workerId}: succeeded sourcify=${result.sourcifyStatus}`,
        createdAtMs: BigInt(Date.now()),
        submittedBy: request.submittedBy,
        eventType: "success",
      });
    } catch (err) {
      const now = BigInt(Date.now());
      const retryable = isRetryable(err);
      const errorCode = isVerifyServiceError(err) ? err.code : "internal_error";
      const errorMessage = err instanceof Error ? err.message : String(err);
      if (retryable && request.attempts <= cfg.verifyMaxRetries) {
        await requeueVerifyRequest({
          id: request.id,
          errorCode,
          errorMessage,
          updatedAtMs: now,
        });
        await appendVerifyJobLog({
          id: randomUUID(),
          requestId: request.id,
          level: "warn",
          message: `worker_${workerId}: retry ${errorCode} ${errorMessage}`,
          createdAtMs: now,
          submittedBy: request.submittedBy,
          eventType: "retry",
        });
      } else {
        await markVerifyRequestFailed({
          id: request.id,
          errorCode,
          errorMessage,
          finishedAtMs: now,
        });
      }
      await appendVerifyJobLog({
        id: randomUUID(),
        requestId: request.id,
        level: "error",
        message: `worker_${workerId}: ${errorCode} ${errorMessage}`,
        createdAtMs: now,
        submittedBy: request.submittedBy,
        eventType: "fail",
      });
    }
  }
}

async function purgeLogsIfNeeded(retentionDays: number): Promise<void> {
  const cutoff = BigInt(Date.now() - retentionDays * 24 * 60 * 60 * 1000);
  await deleteVerifyLogsOlderThan(cutoff);
}

function isRetryable(err: unknown): boolean {
  if (!isVerifyServiceError(err)) {
    return false;
  }
  return err.code === "compiler_unavailable" || err.code === "rpc_unavailable" || err.code === "sourcify_error";
}

function sha256(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

async function runWithTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  let timer: NodeJS.Timeout | null = null;
  const timeoutPromise = new Promise<T>((_, reject) => {
    timer = setTimeout(() => {
      reject(new Error("verify timeout"));
    }, timeoutMs);
  });
  try {
    return await Promise.race([promise, timeoutPromise]);
  } finally {
    if (timer) {
      clearTimeout(timer);
    }
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

if (isMainModule()) {
  main().catch(async (err) => {
    console.error("[verify-worker] fatal", err);
    await closeExplorerPool();
    process.exitCode = 1;
  });
}

function isMainModule(): boolean {
  const entry = process.argv[1];
  if (!entry) {
    return false;
  }
  return import.meta.url === pathToFileURL(entry).href;
}

export const verifyWorkerTestHooks = {
  runWithTimeout,
};
