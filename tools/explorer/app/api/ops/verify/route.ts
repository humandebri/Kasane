// どこで: Verify運用API / 何を: 固定窓(15m/24h)でverifyメトリクスを返却 / なぜ: 本番監視で重い任意窓クエリを避けるため

import { NextResponse } from "next/server";
import { getVerifyMetricsSamplesSince } from "../../../../lib/db";
import { stringifyVerifyWindowSummary, summarizeVerifyWindow } from "../../../../lib/verify/metrics";

const WINDOW_15M = 15n * 60n * 1000n;
const WINDOW_24H = 24n * 60n * 60n * 1000n;

export async function GET() {
  const now = BigInt(Date.now());
  const since24h = now - WINDOW_24H;
  const samples = await getVerifyMetricsSamplesSince(since24h);
  const summary15m = stringifyVerifyWindowSummary(summarizeVerifyWindow(samples, now - WINDOW_15M));
  const summary24h = stringifyVerifyWindowSummary(summarizeVerifyWindow(samples, since24h));
  return NextResponse.json({
    currentQueueDepth: samples[0]?.queueDepth.toString() ?? "0",
    last_15m: summary15m,
    last_24h: summary24h,
  });
}
