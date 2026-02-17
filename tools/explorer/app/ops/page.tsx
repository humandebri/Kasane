// どこで: Opsダッシュボード / 何を: lag/メトリクス/prune/失敗率を表示 / なぜ: 運用監視を単一ページで完結させるため

import { Badge } from "../../components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../components/ui/table";
import { getOpsView } from "../../lib/data";

export const dynamic = "force-dynamic";

export default async function OpsPage() {
  const data = await getOpsView();
  const prune = data.pruneStatus?.status;

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>Heads</CardTitle>
        </CardHeader>
        <CardContent>
          <dl className="grid grid-cols-1 gap-2 text-sm md:grid-cols-[220px_1fr]">
            <dt className="text-muted-foreground">RPC Head</dt>
            <dd>{formatBigInt(data.rpcHead)}</dd>
            <dt className="text-muted-foreground">DB Head</dt>
            <dd>{formatBigInt(data.dbHead)}</dd>
            <dt className="text-muted-foreground">Lag</dt>
            <dd><Badge variant="outline">{formatBigInt(data.lag)}</Badge></dd>
            <dt className="text-muted-foreground">Meta last_head</dt>
            <dd>{formatBigInt(data.metaLastHead)}</dd>
            <dt className="text-muted-foreground">Last ingest at</dt>
            <dd>{formatTimestamp(data.lastIngestAtMs)}</dd>
            <dt className="text-muted-foreground">Pending stall (15m)</dt>
            <dd><Badge variant={data.pendingStall ? "secondary" : "outline"}>{data.pendingStall ? "true" : "false"}</Badge></dd>
          </dl>
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
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Time</TableHead>
                  <TableHead>Queue</TableHead>
                  <TableHead>Submitted</TableHead>
                  <TableHead>Included</TableHead>
                  <TableHead>Dropped</TableHead>
                  <TableHead>Failure Rate</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.series.map((point) => (
                  <TableRow key={point.sampledAtMs.toString()}>
                    <TableCell>{formatTimestamp(point.sampledAtMs)}</TableCell>
                    <TableCell>{point.queueLen.toString()}</TableCell>
                    <TableCell>{point.totalSubmitted.toString()}</TableCell>
                    <TableCell>{point.totalIncluded.toString()}</TableCell>
                    <TableCell>{point.totalDropped.toString()}</TableCell>
                    <TableCell>{(point.failureRate * 100).toFixed(2)}%</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
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
  if (!Number.isFinite(n)) return value.toString();
  return `${new Date(n).toISOString()} (${value.toString()})`;
}
