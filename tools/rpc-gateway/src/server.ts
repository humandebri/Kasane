// どこで: Gateway HTTP層 / 何を: POST受付とJSON-RPC処理を実行 / なぜ: canisterをHTTP直受けさせずGatewayで制限を掛けるため

import http from "node:http";
import { CONFIG } from "./config";
import {
  ERR_INVALID_REQUEST,
  ERR_PARSE,
  JsonRpcRequest,
  JsonRpcResponse,
  makeError,
  parseJsonWithDepthLimit,
  validateRequest,
} from "./jsonrpc";
import { handleRpc } from "./handlers";

export function startServer(): http.Server {
  const server = http.createServer((req, res) => {
    void handleHttp(req, res);
  });
  server.listen(CONFIG.port, CONFIG.host);
  return server;
}

async function handleHttp(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
  setCors(req, res);

  if (req.method === "OPTIONS") {
    res.statusCode = 204;
    res.end();
    return;
  }

  if (req.method !== "POST") {
    res.statusCode = 405;
    writeJson(res, { error: "method not allowed" });
    return;
  }

  let bodyText: string;
  try {
    bodyText = await readBody(req, CONFIG.maxHttpBodySize);
  } catch (err) {
    res.statusCode = 413;
    writeJson(res, { error: err instanceof Error ? err.message : String(err) });
    return;
  }

  let parsed: unknown;
  try {
    parsed = parseJsonWithDepthLimit(bodyText, CONFIG.maxJsonDepth);
  } catch {
    writeRpc(res, makeError(null, ERR_PARSE, "parse error"));
    return;
  }

  if (Array.isArray(parsed)) {
    if (parsed.length === 0) {
      writeRpc(res, makeError(null, ERR_INVALID_REQUEST, "invalid request"));
      return;
    }
    if (parsed.length > CONFIG.maxBatchLen) {
      writeRpc(res, makeError(null, ERR_INVALID_REQUEST, "batch length exceeds limit"));
      return;
    }
    const out = await handleBatch(parsed);
    if (out.length === 0) {
      res.statusCode = 204;
      res.end();
      return;
    }
    writeRpc(res, out);
    return;
  }

  const maybeReq = validateRequest(parsed);
  if (!maybeReq) {
    writeRpc(res, makeError(null, ERR_INVALID_REQUEST, "invalid request"));
    return;
  }
  const single = await handleSingle(maybeReq);
  if (single === null) {
    res.statusCode = 204;
    res.end();
    return;
  }
  writeRpc(res, single);
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

function readBody(req: http.IncomingMessage, maxBytes: number): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    let total = 0;

    req.on("data", (chunk: Buffer) => {
      total += chunk.length;
      if (total > maxBytes) {
        reject(new Error("payload too large"));
        req.destroy();
        return;
      }
      chunks.push(chunk);
    });

    req.on("end", () => {
      resolve(Buffer.concat(chunks).toString("utf8"));
    });

    req.on("error", (err) => {
      reject(err);
    });
  });
}

function setCors(req: http.IncomingMessage, res: http.ServerResponse): void {
  const allowOrigin = resolveCorsAllowOrigin(req.headers.origin, CONFIG.corsOrigins);
  if (allowOrigin) {
    res.setHeader("access-control-allow-origin", allowOrigin);
    res.setHeader("vary", "origin");
  }
  res.setHeader("access-control-allow-methods", "POST, OPTIONS");
  res.setHeader("access-control-allow-headers", "content-type");
}

function resolveCorsAllowOrigin(requestOrigin: string | undefined, allowedOrigins: string[]): string | null {
  if (allowedOrigins.includes("*")) {
    return "*";
  }
  if (!requestOrigin) {
    return null;
  }
  return allowedOrigins.includes(requestOrigin) ? requestOrigin : null;
}

export const __test_resolve_cors_allow_origin = resolveCorsAllowOrigin;

function writeRpc(res: http.ServerResponse, payload: JsonRpcResponse | JsonRpcResponse[]): void {
  res.setHeader("content-type", "application/json");
  res.end(stringifyJson(payload));
}

function writeJson(res: http.ServerResponse, payload: unknown): void {
  res.setHeader("content-type", "application/json");
  res.end(stringifyJson(payload));
}

function stringifyJson(payload: unknown): string {
  return JSON.stringify(payload, (_key: string, value: unknown) => {
    if (typeof value === "bigint") {
      return value.toString(10);
    }
    return value;
  });
}
