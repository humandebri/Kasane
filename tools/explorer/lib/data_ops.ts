// どこで: Explorerデータ層(ops/prune補助) / 何を: prune状態パースとops時系列計算を集約 / なぜ: data.tsの責務を分離して保守しやすくするため

import type { PruneStatusView } from "./rpc";

export type StoredPruneStatus = {
  fetchedAtMs: bigint | null;
  status: {
    pruningEnabled: boolean;
    pruneRunning: boolean;
    needPrune: boolean;
    prunedBeforeBlock: bigint | null;
    oldestKeptBlock: bigint | null;
    oldestKeptTimestamp: bigint | null;
    estimatedKeptBytes: bigint;
    highWaterBytes: bigint;
    lowWaterBytes: bigint;
    hardEmergencyBytes: bigint;
    lastPruneAt: bigint;
  } | null;
};

export type OpsSeriesPoint = {
  sampledAtMs: bigint;
  queueLen: bigint;
  totalSubmitted: bigint;
  totalIncluded: bigint;
  totalDropped: bigint;
  failureRate: number;
};

export function buildOpsSeries(
  samples: Array<{ sampledAtMs: bigint; queueLen: bigint; totalSubmitted: bigint; totalIncluded: bigint; totalDropped: bigint }>
): OpsSeriesPoint[] {
  const asc = [...samples].reverse();
  return asc.map((current, index) => {
    const prev = index > 0 ? asc[index - 1] : null;
    const deltaSubmitted = prev ? current.totalSubmitted - prev.totalSubmitted : 0n;
    const deltaDropped = prev ? current.totalDropped - prev.totalDropped : 0n;
    const denom = deltaSubmitted > 0n ? Number(deltaSubmitted) : 1;
    const numer = deltaDropped > 0n ? Number(deltaDropped) : 0;
    return {
      sampledAtMs: current.sampledAtMs,
      queueLen: current.queueLen,
      totalSubmitted: current.totalSubmitted,
      totalIncluded: current.totalIncluded,
      totalDropped: current.totalDropped,
      failureRate: numer / denom,
    };
  });
}

export function detectPendingStall(series: OpsSeriesPoint[], windowMs: number): boolean {
  if (series.length < 2) {
    return false;
  }
  const newest = series[series.length - 1];
  if (!newest) {
    return false;
  }
  const threshold = newest.sampledAtMs - BigInt(windowMs);
  let oldestInWindow: OpsSeriesPoint | null = null;
  for (const point of series) {
    if (point.sampledAtMs < threshold) {
      continue;
    }
    if (oldestInWindow === null) {
      oldestInWindow = point;
    }
    if (point.queueLen === 0n) {
      return false;
    }
  }
  if (!oldestInWindow) {
    return false;
  }
  return newest.queueLen > 0n && newest.totalIncluded - oldestInWindow.totalIncluded === 0n;
}

export function parseStoredPruneStatus(raw: string | null): StoredPruneStatus | null {
  if (!raw) {
    return null;
  }
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!isRecord(parsed)) {
      return null;
    }
    const statusRaw = parsed.status;
    const fetchedAtMs = toBigIntOrNull(parsed.fetched_at_ms);
    if (!isRecord(statusRaw)) {
      return { fetchedAtMs, status: null };
    }
    const status = parseStoredPruneStatusRecord(statusRaw);
    return {
      fetchedAtMs,
      status,
    };
  } catch {
    return null;
  }
}

export function parseStoredPruneStatusForTest(raw: string | null): StoredPruneStatus | null {
  return parseStoredPruneStatus(raw);
}

export function pruneStatusFromLive(live: PruneStatusView | null): StoredPruneStatus | null {
  if (!live) {
    return null;
  }
  return {
    fetchedAtMs: null,
    status: {
      pruningEnabled: live.pruning_enabled,
      pruneRunning: live.prune_running,
      needPrune: live.need_prune,
      prunedBeforeBlock: live.pruned_before_block.length === 0 ? null : live.pruned_before_block[0],
      oldestKeptBlock: live.oldest_kept_block.length === 0 ? null : live.oldest_kept_block[0],
      oldestKeptTimestamp: live.oldest_kept_timestamp.length === 0 ? null : live.oldest_kept_timestamp[0],
      estimatedKeptBytes: live.estimated_kept_bytes,
      highWaterBytes: live.high_water_bytes,
      lowWaterBytes: live.low_water_bytes,
      hardEmergencyBytes: live.hard_emergency_bytes,
      lastPruneAt: live.last_prune_at,
    },
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function parseStoredPruneStatusRecord(
  statusRaw: Record<string, unknown>
): StoredPruneStatus["status"] {
  const pruningEnabled = toBoolOrNull(statusRaw.pruning_enabled);
  const pruneRunning = toBoolOrNull(statusRaw.prune_running);
  const needPrune = toBoolOrNull(statusRaw.need_prune);
  const estimatedKeptBytes = toBigIntOrNull(statusRaw.estimated_kept_bytes);
  const highWaterBytes = toBigIntOrNull(statusRaw.high_water_bytes);
  const lowWaterBytes = toBigIntOrNull(statusRaw.low_water_bytes);
  const hardEmergencyBytes = toBigIntOrNull(statusRaw.hard_emergency_bytes);
  const lastPruneAt = toBigIntOrNull(statusRaw.last_prune_at);
  if (
    pruningEnabled === null ||
    pruneRunning === null ||
    needPrune === null ||
    estimatedKeptBytes === null ||
    highWaterBytes === null ||
    lowWaterBytes === null ||
    hardEmergencyBytes === null ||
    lastPruneAt === null
  ) {
    return null;
  }
  return {
    pruningEnabled,
    pruneRunning,
    needPrune,
    prunedBeforeBlock: toBigIntOrNull(statusRaw.pruned_before_block),
    oldestKeptBlock: toBigIntOrNull(statusRaw.oldest_kept_block),
    oldestKeptTimestamp: toBigIntOrNull(statusRaw.oldest_kept_timestamp),
    estimatedKeptBytes,
    highWaterBytes,
    lowWaterBytes,
    hardEmergencyBytes,
    lastPruneAt,
  };
}

function toBoolOrNull(value: unknown): boolean | null {
  if (value === true || value === "true" || value === "1") {
    return true;
  }
  if (value === false || value === "false" || value === "0") {
    return false;
  }
  return null;
}

function toBigIntOrNull(value: unknown): bigint | null {
  if (typeof value === "bigint") {
    return value;
  }
  if (typeof value === "number" && Number.isFinite(value) && Number.isInteger(value)) {
    return BigInt(value);
  }
  if (typeof value === "string" && value.trim() !== "") {
    try {
      return BigInt(value);
    } catch {
      return null;
    }
  }
  return null;
}
