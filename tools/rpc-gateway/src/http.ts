// where: gateway HTTP core / what: maps HTTP requests to JSON-RPC responses / why: share behavior between Node and Cloudflare Workers

import { CONFIG } from "./config.js";
import {
  ERR_INVALID_REQUEST,
  ERR_PARSE,
  type JsonRpcRequest,
  type JsonRpcResponse,
  makeError,
  parseJsonWithDepthLimit,
  validateRequest,
} from "./jsonrpc.js";
import { handleRpc } from "./handlers.js";

export type RpcHttpRequest = {
  method: string;
  origin: string | undefined;
  readBodyText: () => Promise<string>;
};

export type RpcHttpResponse = {
  status: number;
  headers: Record<string, string>;
  body: string | null;
};

export async function handleRpcHttp(input: RpcHttpRequest): Promise<RpcHttpResponse> {
  const headers = corsHeaders(input.origin);

  if (input.method === "OPTIONS") {
    return { status: 204, headers, body: null };
  }

  if (input.method !== "POST") {
    return jsonResponse(405, { error: "method not allowed" }, headers);
  }

  let bodyText: string;
  try {
    bodyText = await input.readBodyText();
  } catch (err) {
    return jsonResponse(413, { error: err instanceof Error ? err.message : String(err) }, headers);
  }

  let parsed: unknown;
  try {
    parsed = parseJsonWithDepthLimit(bodyText, CONFIG.maxJsonDepth);
  } catch {
    return rpcResponse(makeError(null, ERR_PARSE, "parse error"), headers);
  }

  if (Array.isArray(parsed)) {
    if (parsed.length === 0) {
      return rpcResponse(makeError(null, ERR_INVALID_REQUEST, "invalid request"), headers);
    }
    if (parsed.length > CONFIG.maxBatchLen) {
      return rpcResponse(makeError(null, ERR_INVALID_REQUEST, "batch length exceeds limit"), headers);
    }
    const out = await handleBatch(parsed);
    if (out.length === 0) {
      return { status: 204, headers, body: null };
    }
    return rpcResponse(out, headers);
  }

  const maybeReq = validateRequest(parsed);
  if (!maybeReq) {
    return rpcResponse(makeError(null, ERR_INVALID_REQUEST, "invalid request"), headers);
  }
  const single = await handleSingle(maybeReq);
  if (single === null) {
    return { status: 204, headers, body: null };
  }
  return rpcResponse(single, headers);
}

async function handleBatch(items: unknown[]): Promise<JsonRpcResponse[]> {
  const out: JsonRpcResponse[] = [];
  for (const item of items) {
    const maybeReq = validateRequest(item);
    if (!maybeReq) {
      out.push(makeError(null, ERR_INVALID_REQUEST, "invalid request"));
      continue;
    }
    const maybeResp = await handleSingle(maybeReq);
    if (maybeResp) {
      out.push(maybeResp);
    }
  }
  return out;
}

async function handleSingle(req: JsonRpcRequest): Promise<JsonRpcResponse | null> {
  const out = await handleRpc(req);
  if (!("id" in req)) {
    return null;
  }
  return out;
}

function corsHeaders(origin: string | undefined): Record<string, string> {
  const headers: Record<string, string> = {
    "access-control-allow-methods": "POST, OPTIONS",
    "access-control-allow-headers": "content-type",
  };
  const allowOrigin = resolveCorsAllowOrigin(origin, CONFIG.corsOrigins);
  if (allowOrigin) {
    headers["access-control-allow-origin"] = allowOrigin;
    headers.vary = "origin";
  }
  return headers;
}

export function resolveCorsAllowOrigin(requestOrigin: string | undefined, allowedOrigins: string[]): string | null {
  if (allowedOrigins.includes("*")) {
    return "*";
  }
  if (!requestOrigin) {
    return null;
  }
  return allowedOrigins.includes(requestOrigin) ? requestOrigin : null;
}

function rpcResponse(payload: JsonRpcResponse | JsonRpcResponse[], headers: Record<string, string>): RpcHttpResponse {
  return jsonResponse(200, payload, headers);
}

function jsonResponse(status: number, payload: unknown, headers: Record<string, string>): RpcHttpResponse {
  return {
    status,
    headers: { ...headers, "content-type": "application/json" },
    body: stringifyJson(payload),
  };
}

export function stringifyJson(payload: unknown): string {
  return JSON.stringify(payload, (_key: string, value: unknown) => {
    if (typeof value === "bigint") {
      return value.toString(10);
    }
    return value;
  });
}
