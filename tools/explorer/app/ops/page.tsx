// どこで: Opsダッシュボード / 何を: lag/メトリクス/prune/失敗率を表示 / なぜ: 運用監視を単一ページで完結させるため

import { Badge } from "../../components/ui/badge";
import { CyclesTrendChart } from "../../components/cycles-trend-chart";
import { OpsTimeseriesTable } from "../../components/ops-timeseries-table";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { getOpsView } from "../../lib/data";

export const dynamic = "force-dynamic";

export default async function OpsPage() {
  const data = await getOpsView();
  const prune = data.pruneStatus?.status;

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>Cycles Trend</CardTitle>
        </CardHeader>
        <CardContent>
          {data.series.length === 0 ? (
            <p className="text-sm text-muted-foreground">No cycle samples.</p>
          ) : (
            <div className="space-y-2">
              <CyclesTrendChart
                points={data.series.map((point) => ({
                  sampledAtMs: point.sampledAtMs.toString(),
                  cycles: point.cycles.toString(),
                }))}
              />
              <div className="flex items-center justify-between text-xs text-slate-500">
                <span>min: {formatCyclesT(getCyclesMin(data.series))}</span>
                <span>max: {formatCyclesT(getCyclesMax(data.series))}</span>
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Ops Timeseries (latest 120)</CardTitle>
        </CardHeader>
        <CardContent>
          {data.series.length === 0 ? (
            <p className="text-sm text-muted-foreground">No ops samples.</p>
          ) : (
            <OpsTimeseriesTable
              points={[...data.series].reverse().map((point) => ({
                sampledAtMs: point.sampledAtMs.toString(),
                queueLen: point.queueLen.toString(),
                cycles: point.cycles.toString(),
                totalSubmitted: point.totalSubmitted.toString(),
                totalIncluded: point.totalIncluded.toString(),
                totalDropped: point.totalDropped.toString(),
                failureRate: point.failureRate,
              }))}
            />
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Daily Metrics</CardTitle>
        </CardHeader>
        <CardContent>
          <dl className="grid grid-cols-1 gap-2 text-sm md:grid-cols-[220px_1fr]">
            <dt className="text-muted-foreground">Latest day</dt>
            <dd>{data.stats.latestDay ?? "N/A"}</dd>
            <dt className="text-muted-foreground">Blocks ingested</dt>
            <dd>{data.stats.latestDayBlocks.toString()}</dd>
            <dt className="text-muted-foreground">Raw bytes</dt>
            <dd>{data.stats.latestDayRawBytes.toString()}</dd>
            <dt className="text-muted-foreground">Compressed bytes</dt>
            <dd>{data.stats.latestDayCompressedBytes.toString()}</dd>
          </dl>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Prune Status</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <dl className="grid grid-cols-1 gap-2 text-sm md:grid-cols-[220px_1fr]">
            <dt className="text-muted-foreground">need_prune (meta)</dt>
            <dd>{formatOptionalBool(data.needPrune)}</dd>
            <dt className="text-muted-foreground">Stored status</dt>
            <dd>{data.pruneStatus ? "available" : "not available"}</dd>
            <dt className="text-muted-foreground">Stored fetched_at</dt>
            <dd>{formatTimestamp(data.pruneStatus?.fetchedAtMs ?? null)}</dd>
            <dt className="text-muted-foreground">Stored pruning_enabled</dt>
            <dd>{formatOptionalBool(prune ? prune.pruningEnabled : null)}</dd>
            <dt className="text-muted-foreground">Stored prune_running</dt>
            <dd>{formatOptionalBool(prune ? prune.pruneRunning : null)}</dd>
            <dt className="text-muted-foreground">Stored pruned_before_block</dt>
            <dd>{formatBigInt(prune ? prune.prunedBeforeBlock : null)}</dd>
            <dt className="text-muted-foreground">Live prune status</dt>
            <dd>{data.pruneStatusLive ? "available" : "not available"}</dd>
          </dl>
        </CardContent>
      </Card>

      {data.warnings.length > 0 ? (
        <Card>
          <CardHeader>
            <CardTitle>Warnings</CardTitle>
          </CardHeader>
          <CardContent>
            <ul className="list-disc pl-5 text-sm">
              {data.warnings.map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          </CardContent>
        </Card>
      ) : null}
    </>
  );
}

function formatBigInt(value: bigint | null): string {
  return value === null ? "N/A" : value.toString();
}

function formatOptionalBool(value: boolean | null): string {
  return value === null ? "N/A" : value ? "true" : "false";
}

function formatTimestamp(value: bigint | null): string {
  if (value === null) return "N/A";
  const n = Number(value);
  if (!Number.isFinite(n)) return "N/A";
  return new Date(n).toLocaleString();
}

function formatCyclesT(value: bigint | null): string {
  if (value === null) {
    return "N/A";
  }
  const trillion = 1_000_000_000_000n;
  const integer = value / trillion;
  const fraction = value % trillion;
  const fractionText = fraction.toString().padStart(12, "0").slice(0, 4);
  return `${integer.toString()}.${fractionText} T`;
}

function getCyclesMin(series: Array<{ cycles: bigint }>): bigint | null {
  if (series.length === 0) {
    return null;
  }
  let out = series[0]?.cycles ?? 0n;
  for (const point of series) {
    if (point.cycles < out) {
      out = point.cycles;
    }
  }
  return out;
}

function getCyclesMax(series: Array<{ cycles: bigint }>): bigint | null {
  if (series.length === 0) {
    return null;
  }
  let out = series[0]?.cycles ?? 0n;
  for (const point of series) {
    if (point.cycles > out) {
      out = point.cycles;
    }
  }
  return out;
}
