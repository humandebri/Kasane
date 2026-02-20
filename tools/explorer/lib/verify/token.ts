// どこで: verify認証トークン層 / 何を: submit用HMACトークン生成 / なぜ: deploy直後の自動submitを安全に共通化するため

import { createHmac } from "node:crypto";

export type VerifyAuthTokenInput = {
  kid: string;
  secret: string;
  sub: string;
  scope: string;
  expSec: number;
  jti: string;
};

export function buildVerifyAuthToken(input: VerifyAuthTokenInput): string {
  const header = base64UrlEncodeJson({ alg: "HS256", typ: "JWT", kid: input.kid });
  const payload = base64UrlEncodeJson({
    sub: input.sub,
    exp: input.expSec,
    scope: input.scope,
    jti: input.jti,
  });
  const message = `${header}.${payload}`;
  const signature = createHmac("sha256", input.secret).update(message).digest("base64url");
  return `${message}.${signature}`;
}

function base64UrlEncodeJson(value: Record<string, string | number>): string {
  return Buffer.from(JSON.stringify(value), "utf8").toString("base64url");
}
