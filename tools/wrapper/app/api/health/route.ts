// どこで: health API route / 何を: canister疎通と設定値の簡易確認を返却 / なぜ: 運用時の初期切り分けを簡単にするため

import { NextResponse } from "next/server";
import { toApiError, toErrorBody } from "@/lib/errors";
import { getHealth, type HealthDeps } from "@/lib/server";
import { getHealthDepsOverride } from "@/lib/route-test-overrides";

export async function GET(): Promise<Response> {
  try {
    const deps: HealthDeps | undefined = getHealthDepsOverride() ?? undefined;
    const result = await getHealth(deps);
    return NextResponse.json(result, { status: result.ok ? 200 : 503 });
  } catch (error) {
    const apiError = toApiError(error, "health_failed");
    return NextResponse.json(toErrorBody(apiError), { status: apiError.status });
  }
}
