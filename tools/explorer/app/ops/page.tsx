// どこで: 運用ダッシュボード / 何を: lag/メトリクス/prune状態を集約表示 / なぜ: 監視向け情報をHomeから分離して確認しやすくするため

import { Badge } from "../../components/ui/badge";
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
          <CardTitle>Heads</CardTitle>
        </CardHeader>
        <CardContent>
          <dl className="grid grid-cols-1 gap-2 text-sm md:grid-cols-[220px_1fr]">
            <dt className="text-muted-foreground">RPC Head</dt>
            <dd>{formatBigInt(data.rpcHead)}</dd>
            <dt className="text-muted-foreground">DB Head</dt>
            <dd>{formatBigInt(data.dbHead)}</dd>
            <dt className="text-muted-foreground">Lag</dt>
            <dd>
              <Badge variant="outline">{formatBigInt(data.lag)}</Badge>
            </dd>
            <dt className="text-muted-foreground">Meta last_head</dt>
            <dd>{formatBigInt(data.metaLastHead)}</dd>
            <dt className="text-muted-foreground">Last ingest at</dt>
            <dd>{formatTimestamp(data.lastIngestAtMs)}</dd>
          </dl>
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
            <dt className="text-muted-foreground">Total blocks</dt>
            <dd>{data.stats.totalBlocks.toString()}</dd>
            <dt className="text-muted-foreground">Total txs</dt>
            <dd>{data.stats.totalTxs.toString()}</dd>
          </dl>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Prune Status</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <dl className="grid grid-cols-1 gap-2 text-sm md:grid-cols-[220px_1fr]">
            <dt className="text-muted-foreground">need_prune (meta)</dt>
            <dd>{formatOptionalBool(data.needPrune)}</dd>
            <dt className="text-muted-foreground">Stored prune_status</dt>
            <dd>
              {data.pruneStatus
                ? data.pruneStatus.fetchedAtMs === null
                  ? "available (from live fallback)"
                  : "available"
                : "not available"}
            </dd>
            <dt className="text-muted-foreground">Stored fetched_at</dt>
            <dd>{formatTimestamp(data.pruneStatus?.fetchedAtMs ?? null)}</dd>
            <dt className="text-muted-foreground">Stored pruning_enabled</dt>
            <dd>{formatOptionalBool(prune ? prune.pruningEnabled : null)}</dd>
            <dt className="text-muted-foreground">Stored prune_running</dt>
            <dd>{formatOptionalBool(prune ? prune.pruneRunning : null)}</dd>
            <dt className="text-muted-foreground">Stored pruned_before_block</dt>
            <dd>{formatBigInt(prune ? prune.prunedBeforeBlock : null)}</dd>
            <dt className="text-muted-foreground">Stored estimated_kept_bytes</dt>
            <dd>{prune ? prune.estimatedKeptBytes.toString() : "N/A"}</dd>
            <dt className="text-muted-foreground">Live prune status</dt>
            <dd>{data.pruneStatusLive ? "available" : "not available"}</dd>
            <dt className="text-muted-foreground">Live need_prune</dt>
            <dd>{formatOptionalBool(data.pruneStatusLive ? data.pruneStatusLive.need_prune : null)}</dd>
          </dl>

          {data.pruneStatus ? (
            <details className="rounded-md border p-3 text-sm">
              <summary className="cursor-pointer font-medium">Stored prune_status raw JSON</summary>
              <pre className="mt-2 overflow-x-auto text-xs">{JSON.stringify(data.pruneStatus, null, 2)}</pre>
            </details>
          ) : null}
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
  if (value === null) {
    return "N/A";
  }
  const asNumber = Number(value);
  if (!Number.isFinite(asNumber)) {
    return value.toString();
  }
  return `${new Date(asNumber).toISOString()} (${value.toString()})`;
}
