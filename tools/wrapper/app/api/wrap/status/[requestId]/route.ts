// どこで: status API route / 何を: dispatch/execution状態を統合返却 / なぜ: 利用側が2 canisterを意識せず照会できるようにするため

import { NextResponse } from "next/server";
import { toApiError, toErrorBody } from "@/lib/errors";
import { getRequestStatus, type StatusDeps } from "@/lib/server";
import { assertValidRequestIdHex } from "@/lib/validate";
import { getStatusDepsOverride } from "@/lib/route-test-overrides";

export async function GET(_request: Request, context: { params: Promise<{ requestId: string }> }): Promise<Response> {
  try {
    const { requestId } = await context.params;
    assertValidRequestIdHex(requestId);
    const deps: StatusDeps | undefined = getStatusDepsOverride() ?? undefined;
    const result = await getRequestStatus(requestId, deps);
    return NextResponse.json(result, { status: 200 });
  } catch (error) {
    const apiError = toApiError(error, "status_failed");
    return NextResponse.json(toErrorBody(apiError), { status: apiError.status });
  }
}
