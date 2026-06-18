// where: gateway HTTP layer / what: accepts POST and processes JSON-RPC / why: enforce gateway-side limits instead of exposing canister HTTP directly

import http from "node:http";
import { CONFIG } from "./config.js";
import { configureGateway } from "./config.js";
import { handleRpcHttp, resolveCorsAllowOrigin } from "./http.js";

export function startServer(): http.Server {
  configureGateway(process.env, { requireCanisterId: true });
  const server = http.createServer((req, res) => {
    void handleHttp(req, res);
  });
  server.listen(CONFIG.port, CONFIG.host);
  return server;
}

async function handleHttp(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
  const response = await handleRpcHttp({
    method: req.method ?? "",
    origin: req.headers.origin,
    readBodyText: () => readBody(req, CONFIG.maxHttpBodySize),
  });
  res.statusCode = response.status;
  for (const [key, value] of Object.entries(response.headers)) {
    res.setHeader(key, value);
  }
  if (response.body === null) {
    res.end();
    return;
  }
  res.end(response.body);
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

export const __test_resolve_cors_allow_origin = resolveCorsAllowOrigin;
