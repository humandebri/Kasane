// どこで: verifyメトリクス共通層 / 何を: 窓集計と型変換を共通化 / なぜ: APIと画面でのロジック重複を防ぐため

export type VerifyMetricsSampleLike = {
  sampledAtMs: bigint;
  successCount: bigint;
  failedCount: bigint;
  p50DurationMs: bigint | null;
  p95DurationMs: bigint | null;
  failByCodeJson: string;
};

export type VerifyWindowSummaryBigInt = {
  successCount: bigint;
  failedCount: bigint;
  successRate: number | null;
  p50DurationMs: bigint | null;
  p95DurationMs: bigint | null;
  failTopCodes: Array<{ code: string; count: bigint }>;
};

export function summarizeVerifyWindow(
  samples: readonly VerifyMetricsSampleLike[],
  sinceMs: bigint
): VerifyWindowSummaryBigInt {
  const window = samples.filter((sample) => sample.sampledAtMs >= sinceMs);
  let successCount = 0n;
  let failedCount = 0n;
  const p50s: bigint[] = [];
  const p95s: bigint[] = [];
  const failCounts = new Map<string, bigint>();

  for (const sample of window) {
    successCount += sample.successCount;
    failedCount += sample.failedCount;
    if (sample.p50DurationMs !== null) {
      p50s.push(sample.p50DurationMs);
    }
    if (sample.p95DurationMs !== null) {
      p95s.push(sample.p95DurationMs);
    }
    try {
      const parsed = JSON.parse(sample.failByCodeJson) as Record<string, string>;
      for (const [code, countRaw] of Object.entries(parsed)) {
        const prev = failCounts.get(code) ?? 0n;
        failCounts.set(code, prev + BigInt(countRaw));
      }
    } catch {
      // no-op: 過去データの壊れ行は集計継続
    }
  }

  const total = successCount + failedCount;
  const successRate = total > 0n ? Number((successCount * 10_000n) / total) / 100 : null;
  const failTopCodes = [...failCounts.entries()]
    .sort((a, b) => (a[1] < b[1] ? 1 : a[1] > b[1] ? -1 : 0))
    .slice(0, 5)
    .map(([code, count]) => ({ code, count }));

  return {
    successCount,
    failedCount,
    successRate,
    p50DurationMs: medianBigInt(p50s),
    p95DurationMs: medianBigInt(p95s),
    failTopCodes,
  };
}

export function medianBigInt(values: readonly bigint[]): bigint | null {
  if (values.length === 0) {
    return null;
  }
  const sorted = [...values].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  return sorted[Math.floor(sorted.length / 2)] ?? null;
}

export function stringifyVerifyWindowSummary(summary: VerifyWindowSummaryBigInt): {
  successCount: string;
  failedCount: string;
  successRate: number | null;
  p50DurationMs: string | null;
  p95DurationMs: string | null;
  failTopCodes: Array<{ code: string; count: string }>;
} {
  return {
    successCount: summary.successCount.toString(),
    failedCount: summary.failedCount.toString(),
    successRate: summary.successRate,
    p50DurationMs: summary.p50DurationMs === null ? null : summary.p50DurationMs.toString(),
    p95DurationMs: summary.p95DurationMs === null ? null : summary.p95DurationMs.toString(),
    failTopCodes: summary.failTopCodes.map((item) => ({
      code: item.code,
      count: item.count.toString(),
    })),
  };
}
