// どこで: Verify申請API / 何を: 検証ジョブ投入と重複抑止 / なぜ: 重いコンパイル処理を非同期キューへ分離するため

import { randomUUID } from "node:crypto";
import { NextResponse, type NextRequest } from "next/server";
import {
  appendVerifyJobLog,
  countVerifyRequestsByUserSince,
} from "../../../../lib/db";
import { loadConfig } from "../../../../lib/config";
import { authenticateVerifyRequest, hashAuditValue } from "../../../../lib/verify/auth";
import {
  canonicalizeVerifyInput,
  compressVerifyPayload,
  hashVerifyInput,
  normalizeVerifySubmitInput,
} from "../../../../lib/verify/normalize";
import { createOrGetVerifyRequest } from "../../../../lib/verify/submit";

export async function POST(request: NextRequest) {
  const cfg = loadConfig(process.env);
  if (!cfg.verifyEnabled) {
    return NextResponse.json({ error: "verify is disabled" }, { status: 503 });
  }
  const auth = await authenticateVerifyRequest(request, { consumeReplay: true });
  if (!auth) {
    return NextResponse.json({ error: "unauthorized" }, { status: 401 });
  }

  let normalized;
  try {
    const raw = await request.json();
    normalized = normalizeVerifySubmitInput(raw);
  } catch (err) {
    const message = err instanceof Error ? err.message : "invalid json";
    return NextResponse.json({ error: `invalid_input: ${message}` }, { status: 400 });
  }

  if (
    cfg.verifyAllowedCompilerVersions.length > 0 &&
    !cfg.verifyAllowedCompilerVersions.includes(normalized.compilerVersion)
  ) {
    return NextResponse.json({ error: "compiler version is not allowed" }, { status: 400 });
  }

  const nowMs = BigInt(Date.now());
  const [hourlyCount, dailyCount] = await Promise.all([
    countVerifyRequestsByUserSince(auth.userId, nowMs - 60n * 60n * 1000n),
    countVerifyRequestsByUserSince(auth.userId, nowMs - 24n * 60n * 60n * 1000n),
  ]);
  if (hourlyCount >= cfg.verifyHourlyLimit || dailyCount >= cfg.verifyDailyLimit) {
    return NextResponse.json(
      { error: "rate limit exceeded" },
      { status: 429, headers: { "Retry-After": "60" } }
    );
  }

  const canonical = canonicalizeVerifyInput(normalized);
  const rawPayloadBytes = Buffer.byteLength(canonical, "utf8");
  if (rawPayloadBytes > cfg.verifyRawPayloadLimitBytes) {
    return NextResponse.json(
      { error: `payload too large: ${rawPayloadBytes} > ${cfg.verifyRawPayloadLimitBytes}` },
      { status: 413 }
    );
  }

  const inputHash = hashVerifyInput(canonical);
  const id = randomUUID();
  const result = await createOrGetVerifyRequest({
    inputHash,
    id,
    contractAddress: normalized.contractAddress,
    chainId: normalized.chainId,
    submittedBy: auth.userId,
    payloadCompressed: compressVerifyPayload(canonical),
    createdAtMs: nowMs,
  });
  if (result.created) {
    await appendVerifyJobLog({
      id: randomUUID(),
      requestId: result.requestId,
      level: "info",
      message: "verify request queued",
      createdAtMs: nowMs,
      submittedBy: auth.userId,
      ipHash: hashAuditValue(getClientIp(request), cfg.verifyAuditHashSaltCurrent),
      uaHash: hashAuditValue(request.headers.get("user-agent"), cfg.verifyAuditHashSaltCurrent),
      eventType: "submit",
    });
  }
  return NextResponse.json({ requestId: result.requestId, status: result.status }, { status: result.httpStatus });
}

function getClientIp(request: NextRequest): string | null {
  const forwarded = request.headers.get("x-forwarded-for");
  if (forwarded) {
    const first = forwarded.split(",")[0]?.trim();
    if (first) {
      return first;
    }
  }
  const realIp = request.headers.get("x-real-ip");
  return realIp?.trim() || null;
}
