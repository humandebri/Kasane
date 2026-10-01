// どこで: Verify状態API / 何を: 申請者向けにジョブ状態を返す / なぜ: 非同期処理の進捗を安全に可視化するため

import { getVerifyRequestById } from "../db";
import { loadConfig } from "../config";
import { authenticateVerifyRequest } from "../verify/auth";

export async function GET(request: Request) {
  const cfg = loadConfig(process.env);
  if (!cfg.verifyEnabled) {
    return Response.json({ error: "verify is disabled" }, { status: 503 });
  }
  const auth = await authenticateVerifyRequest(request, { consumeReplay: false });
  if (!auth) {
    return Response.json({ error: "unauthorized" }, { status: 401 });
  }
  const requestId = new URL(request.url).searchParams.get("id");
  if (!requestId) {
    return Response.json({ error: "id is required" }, { status: 400 });
  }
  const found = await getVerifyRequestById(requestId);
  if (!found) {
    return Response.json({ error: "not found" }, { status: 404 });
  }
  if (found.submittedBy !== auth.userId && !auth.isAdmin) {
    return Response.json({ error: "forbidden" }, { status: 403 });
  }
  return Response.json({
    requestId: found.id,
    status: found.status,
    errorCode: found.errorCode,
    errorMessage: found.errorMessage,
    verifiedContractId: found.verifiedContractId,
  });
}
