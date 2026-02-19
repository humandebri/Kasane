// どこで: Verify状態API / 何を: 申請者向けにジョブ状態を返す / なぜ: 非同期処理の進捗を安全に可視化するため

import { NextResponse, type NextRequest } from "next/server";
import { getVerifyRequestById } from "../../../../lib/db";
import { loadConfig } from "../../../../lib/config";
import { authenticateVerifyRequest } from "../../../../lib/verify/auth";

export async function GET(request: NextRequest) {
  const cfg = loadConfig(process.env);
  if (!cfg.verifyEnabled) {
    return NextResponse.json({ error: "verify is disabled" }, { status: 503 });
  }
  const auth = await authenticateVerifyRequest(request, { consumeReplay: false });
  if (!auth) {
    return NextResponse.json({ error: "unauthorized" }, { status: 401 });
  }
  const requestId = request.nextUrl.searchParams.get("id");
  if (!requestId) {
    return NextResponse.json({ error: "id is required" }, { status: 400 });
  }
  const found = await getVerifyRequestById(requestId);
  if (!found) {
    return NextResponse.json({ error: "not found" }, { status: 404 });
  }
  if (found.submittedBy !== auth.userId && !auth.isAdmin) {
    return NextResponse.json({ error: "forbidden" }, { status: 403 });
  }
  return NextResponse.json({
    requestId: found.id,
    status: found.status,
    errorCode: found.errorCode,
    errorMessage: found.errorMessage,
    verifiedContractId: found.verifiedContractId,
  });
}
