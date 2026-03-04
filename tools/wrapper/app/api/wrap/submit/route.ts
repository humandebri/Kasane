// どこで: submit API route / 何を: unwrap送信をBFFで受けてwrapperへ転送 / なぜ: ブラウザ直接呼び出しを排除し契約を固定するため

import { NextResponse } from "next/server";
import { toApiError, toErrorBody } from "@/lib/errors";
import { submitUnwrapRequest, type SubmitDeps } from "@/lib/server";
import { parseSubmitPayload } from "@/lib/validate";
import { getSubmitDepsOverride } from "@/lib/route-test-overrides";

export async function POST(request: Request): Promise<Response> {
  try {
    const body = await request.json();
    const payload = parseSubmitPayload(body);
    const deps: SubmitDeps | undefined = getSubmitDepsOverride() ?? undefined;
    const result = await submitUnwrapRequest(payload, deps);
    return NextResponse.json(result, { status: 200 });
  } catch (error) {
    const apiError = toApiError(error, "submit_failed");
    return NextResponse.json(toErrorBody(apiError), { status: apiError.status });
  }
}
