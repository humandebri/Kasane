// どこで: Opsダッシュボード / 何を: lag/メトリクス/prune/失敗率を表示 / なぜ: 運用監視を単一ページで完結させるため

import { Badge } from "../../components/ui/badge";
import { CapacityTrendChart } from "../../components/capacity-trend-chart";
import { CyclesTrendChart } from "../../components/cycles-trend-chart";
import { OpsTimeseriesTable } from "../../components/ops-timeseries-table";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import Link from "next/link";
import { getOpsView, parseCyclesTrendWindow } from "../../lib/data";

export const dynamic = "force-dynamic";

export default async function OpsPage({
  searchParams,
}: {
  searchParams: Promise<{ trend?: string }>;
}) {
  const params = await searchParams;
  const trend = parseCyclesTrendWindow(params.trend);
  const data = await getOpsView(trend);
  const prune = data.pruneStatus?.status;

  return (
    <>
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-2">
            <CardTitle>Cycles Trend</CardTitle>
            <div className="flex items-center gap-2 text-xs">
              <Link
                href="/ops?trend=24h"
                className={`rounded-full border px-3 py-1 ${trend === "24h" ? "border-sky-300 bg-sky-50 text-sky-700" : "border-slate-300 bg-white text-slate-700"}`}
              >
                24h
              </Link>
              <Link
                href="/ops?trend=7d"
                className={`rounded-full border px-3 py-1 ${trend === "7d" ? "border-sky-300 bg-sky-50 text-sky-700" : "border-slate-300 bg-white text-slate-700"}`}
              >
                7d
              </Link>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {data.cyclesTrendSeries.length === 0 ? (
            <p className="text-sm text-muted-foreground">No cycle samples in the selected window.</p>
          ) : (
            <div className="space-y-2">
              <CyclesTrendChart
                points={data.cyclesTrendSeries.map((point) => ({
                  sampledAtMs: point.sampledAtMs.toString(),
                  cycles: point.cycles.toString(),
                }))}
              />
              <div className="flex items-center justify-between text-xs text-slate-500">
                <span>min: {formatCyclesT(getCyclesMin(data.cyclesTrendSeries))}</span>
                <span>max: {formatCyclesT(getCyclesMax(data.cyclesTrendSeries))}</span>
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Ops Timeseries (latest 10)</CardTitle>
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

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
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
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Prune History (latest 10 changes)</CardTitle>
        </CardHeader>
        <CardContent>
          {data.pruneHistory.length === 0 ? (
            <p className="text-sm text-muted-foreground">No prune history yet.</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="min-w-full text-sm">
                <thead>
                  <tr className="border-b text-left">
                    <th className="py-2 pr-4 font-medium text-muted-foreground">Sampled At</th>
                    <th className="py-2 pr-4 font-medium text-muted-foreground">pruned_before_block</th>
                  </tr>
                </thead>
                <tbody>
                  {data.pruneHistory.map((row) => (
                    <tr key={`${row.sampledAtMs.toString()}:${row.prunedBeforeBlock.toString()}`} className="border-b last:border-0">
                      <td className="py-2 pr-4">{formatTimestamp(row.sampledAtMs)}</td>
                      <td className="py-2 pr-4">{row.prunedBeforeBlock.toString()}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Canister Capacity</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {data.capacityTrendSeries.length === 0 ? (
            <p className="text-sm text-muted-foreground">No capacity trend samples yet.</p>
          ) : (
            <CapacityTrendChart
              points={data.capacityTrendSeries.map((point) => ({
                sampledAtMs: point.sampledAtMs.toString(),
                estimatedKeptBytes: point.estimatedKeptBytes.toString(),
                highWaterBytes: point.highWaterBytes.toString(),
                hardEmergencyBytes: point.hardEmergencyBytes.toString(),
              }))}
            />
          )}
          <dl className="grid grid-cols-1 gap-2 text-sm md:grid-cols-[220px_1fr]">
            <dt className="text-muted-foreground">Estimated kept (MB)</dt>
            <dd>{formatMegaBytes(data.capacity.estimatedKeptBytes)}</dd>
            <dt className="text-muted-foreground">Low water (MB)</dt>
            <dd>{formatMegaBytes(data.capacity.lowWaterBytes)}</dd>
            <dt className="text-muted-foreground">High water (MB)</dt>
            <dd>{formatMegaBytes(data.capacity.highWaterBytes)}</dd>
            <dt className="text-muted-foreground">Hard emergency (MB)</dt>
            <dd>{formatMegaBytes(data.capacity.hardEmergencyBytes)}</dd>
            <dt className="text-muted-foreground">Growth (24h)</dt>
            <dd>{formatMegaBytesPerDay(data.capacity.forecast24h.growthBytesPerDay)}</dd>
            <dt className="text-muted-foreground">Days to high water (24h)</dt>
            <dd>{formatDays(data.capacity.forecast24h.daysToHighWater)}</dd>
            <dt className="text-muted-foreground">Days to hard emergency (24h)</dt>
            <dd>{formatDays(data.capacity.forecast24h.daysToHardEmergency)}</dd>
            <dt className="text-muted-foreground">Growth (7d)</dt>
            <dd>{formatMegaBytesPerDay(data.capacity.forecast7d.growthBytesPerDay)}</dd>
            <dt className="text-muted-foreground">Days to high water (7d)</dt>
            <dd>{formatDays(data.capacity.forecast7d.daysToHighWater)}</dd>
            <dt className="text-muted-foreground">Days to hard emergency (7d)</dt>
            <dd>{formatDays(data.capacity.forecast7d.daysToHardEmergency)}</dd>
          </dl>
          <div className="space-y-3">
            <div>
              <div className="mb-1 flex items-center justify-between text-xs text-slate-600">
                <span>Usage vs High water</span>
                <span>{formatPercent(data.capacity.highWaterRatio)}</span>
              </div>
              <div className="h-2 rounded bg-slate-100">
                <div
                  className="h-2 rounded bg-amber-500"
                  style={{ width: `${clampPercent(data.capacity.highWaterRatio)}%` }}
                />
              </div>
            </div>
            <div>
              <div className="mb-1 flex items-center justify-between text-xs text-slate-600">
                <span>Usage vs Hard emergency</span>
                <span>{formatPercent(data.capacity.hardEmergencyRatio)}</span>
              </div>
              <div className="h-2 rounded bg-slate-100">
                <div
                  className="h-2 rounded bg-rose-500"
                  style={{ width: `${clampPercent(data.capacity.hardEmergencyRatio)}%` }}
                />
              </div>
            </div>
          </div>
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

function formatMegaBytes(value: bigint | null): string {
  if (value === null) return "N/A";
  const bytes = Number(value);
  if (!Number.isFinite(bytes) || bytes < 0) return value.toString();
  const mb = bytes / (1024 * 1024);
  return `${mb.toFixed(2)} MB`;
}

function formatPercent(ratio: number | null): string {
  if (ratio === null || !Number.isFinite(ratio)) return "N/A";
  return `${(ratio * 100).toFixed(2)}%`;
}

function clampPercent(ratio: number | null): number {
  if (ratio === null || !Number.isFinite(ratio)) return 0;
  const value = ratio * 100;
  if (value < 0) return 0;
  if (value > 100) return 100;
  return value;
}

function formatMegaBytesPerDay(bytesPerDay: number | null): string {
  if (bytesPerDay === null || !Number.isFinite(bytesPerDay)) return "N/A";
  const mbPerDay = bytesPerDay / (1024 * 1024);
  return `${mbPerDay.toFixed(2)} MB/day`;
}

function formatDays(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return "N/A";
  if (value <= 0) return "0.00 days";
  return `${value.toFixed(2)} days`;
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
