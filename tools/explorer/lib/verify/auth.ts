// どこで: verify API認証 / 何を: HMACトークン(kid/sub/exp/scope/jti)を検証 / なぜ: 本番運用での認可・リプレイ対策を満たすため

import { createHmac, timingSafeEqual } from "node:crypto";
import type { NextRequest } from "next/server";
import { loadConfig } from "../config";
import { consumeVerifyReplayJti } from "../db";

export type VerifyAuthContext = {
  userId: string;
  isAdmin: boolean;
  tokenKid: string;
  scope: string;
};

type VerifyTokenPayload = {
  sub: string;
  exp: number;
  scope: string;
  jti: string;
};

type AuthenticateVerifyRequestOptions = {
  consumeReplay?: boolean;
};

export async function authenticateVerifyRequest(
  request: NextRequest,
  options: AuthenticateVerifyRequestOptions = {}
): Promise<VerifyAuthContext | null> {
  const cfg = loadConfig(process.env);
  const consumeReplay = options.consumeReplay ?? true;
  const authHeader = request.headers.get("authorization");
  if (!authHeader || !authHeader.startsWith("Bearer ")) {
    return null;
  }
  const token = authHeader.slice("Bearer ".length).trim();
  if (!token) {
    return null;
  }

  const parsed = parseCompactToken(token);
  if (parsed && cfg.verifyAuthHmacKeys.size > 0) {
    const secret = cfg.verifyAuthHmacKeys.get(parsed.kid);
    if (!secret) {
      return null;
    }
    const expected = signToken(`${parsed.headerB64}.${parsed.payloadB64}`, secret);
    if (!constantTimeEqual(expected, parsed.sigB64)) {
      return null;
    }
    const payload = parsePayload(parsed.payloadB64);
    if (!payload) {
      return null;
    }
    const nowSec = Math.floor(Date.now() / 1000);
    if (payload.exp <= nowSec) {
      return null;
    }
    if (payload.scope !== cfg.verifyRequiredScope) {
      return null;
    }
    if (consumeReplay) {
      const consumed = await consumeVerifyReplayJti({
        jti: payload.jti,
        sub: payload.sub,
        scope: payload.scope,
        expSec: BigInt(payload.exp),
        consumedAtMs: BigInt(Date.now()),
      });
      if (!consumed) {
        return null;
      }
    }
    return {
      userId: payload.sub,
      isAdmin: cfg.verifyAdminUsers.has(payload.sub),
      tokenKid: parsed.kid,
      scope: payload.scope,
    };
  }

  return null;
}

export function hashAuditValue(raw: string | null, saltCurrent: string): string | null {
  if (!raw || !saltCurrent) {
    return null;
  }
  return createHmac("sha256", saltCurrent).update(raw).digest("hex");
}

export function matchesAuditHash(raw: string, expectedHash: string, saltCurrent: string, saltPrevious: string | null): boolean {
  const current = hashAuditValue(raw, saltCurrent);
  if (current && constantTimeEqual(current, expectedHash)) {
    return true;
  }
  if (!saltPrevious) {
    return false;
  }
  const previous = hashAuditValue(raw, saltPrevious);
  return previous ? constantTimeEqual(previous, expectedHash) : false;
}

function parseCompactToken(token: string): { headerB64: string; payloadB64: string; sigB64: string; kid: string } | null {
  const parts = token.split(".");
  if (parts.length !== 3) {
    return null;
  }
  const [headerB64, payloadB64, sigB64] = parts;
  if (!headerB64 || !payloadB64 || !sigB64) {
    return null;
  }
  const headerJson = decodeBase64Url(headerB64);
  if (!headerJson) {
    return null;
  }
  const parsedHeader = safeParseJson(headerJson);
  if (!isRecord(parsedHeader) || parsedHeader.alg !== "HS256" || typeof parsedHeader.kid !== "string") {
    return null;
  }
  return { headerB64, payloadB64, sigB64, kid: parsedHeader.kid };
}

function parsePayload(payloadB64: string): VerifyTokenPayload | null {
  const payloadJson = decodeBase64Url(payloadB64);
  if (!payloadJson) {
    return null;
  }
  const parsed = safeParseJson(payloadJson);
  if (!isRecord(parsed)) {
    return null;
  }
  if (
    typeof parsed.sub !== "string" ||
    typeof parsed.scope !== "string" ||
    typeof parsed.jti !== "string" ||
    typeof parsed.exp !== "number" ||
    !Number.isInteger(parsed.exp)
  ) {
    return null;
  }
  return {
    sub: parsed.sub,
    scope: parsed.scope,
    jti: parsed.jti,
    exp: parsed.exp,
  };
}

function signToken(message: string, secret: string): string {
  const sig = createHmac("sha256", secret).update(message).digest("base64url");
  return sig;
}

function constantTimeEqual(left: string, right: string): boolean {
  const leftBytes = Buffer.from(left);
  const rightBytes = Buffer.from(right);
  if (leftBytes.length !== rightBytes.length) {
    return false;
  }
  return timingSafeEqual(leftBytes, rightBytes);
}

function decodeBase64Url(value: string): string | null {
  try {
    return Buffer.from(value, "base64url").toString("utf8");
  } catch {
    return null;
  }
}

function safeParseJson(value: string): unknown {
  try {
    return JSON.parse(value);
  } catch {
    return null;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
