// どこで: withdraw API route / 何を: 回収可能requestの返金実行をBFF経由で行う / なぜ: 認可付きupdate callをサーバー側に固定するため

import { NextResponse } from "next/server";
import { toApiError, toErrorBody } from "@/lib/errors";
import { withdrawRequest, type WithdrawDeps } from "@/lib/server";
import { assertValidRequestIdHex } from "@/lib/validate";
import { getWithdrawDepsOverride } from "@/lib/route-test-overrides";

export async function POST(request: Request): Promise<Response> {
  try {
    const body = await request.json();
    const requestId =
      typeof body?.requestId === "string" ? body.requestId : "";
    assertValidRequestIdHex(requestId);
    const deps: WithdrawDeps | undefined = getWithdrawDepsOverride() ?? undefined;
    const result = await withdrawRequest(requestId, deps);
    return NextResponse.json(result, { status: 200 });
  } catch (error) {
    const apiError = toApiError(error, "withdraw_failed");
    return NextResponse.json(toErrorBody(apiError), { status: apiError.status });
  }
}

