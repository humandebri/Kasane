// どこで: health API route / 何を: canister疎通と設定値の簡易確認を返却 / なぜ: 運用時の初期切り分けを簡単にするため

import { NextResponse } from "next/server";
import { getDispatchStatus } from "@/lib/canister/wrapper-client";
import { getExecutionResult } from "@/lib/canister/wrap-client";
import { loadConfig } from "@/lib/config";
import { toApiError, toErrorBody } from "@/lib/errors";

export async function GET(): Promise<Response> {
  try {
    const cfg = loadConfig();
    const probe = new Uint8Array(32);
    const [gatewayProbe, wrapProbe] = await Promise.allSettled([
      getDispatchStatus(probe),
      getExecutionResult(probe),
    ]);

    const result = {
      ok: gatewayProbe.status === "fulfilled" && wrapProbe.status === "fulfilled",
      evmGatewayReachable: gatewayProbe.status === "fulfilled",
      wrapReachable: wrapProbe.status === "fulfilled",
      config: {
        icHost: cfg.icHost,
        evmGatewayCanisterId: cfg.evmGatewayCanisterId,
        wrapCanisterId: cfg.wrapCanisterId,
      },
    };
    return NextResponse.json(result, { status: result.ok ? 200 : 503 });
  } catch (error) {
    const apiError = toApiError(error, "health_failed");
    return NextResponse.json(toErrorBody(apiError), { status: apiError.status });
  }
}
