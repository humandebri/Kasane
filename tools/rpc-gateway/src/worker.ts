// where: Cloudflare Workers entrypoint / what: serves JSON-RPC through Fetch / why: remove VPS HTTP server dependency

import { CONFIG, configureGateway } from "./config.js";
import { handleRpcHttp } from "./http.js";

type GatewayWorkerEnv = Record<string, string | undefined>;

export default {
  async fetch(request: Request, env: GatewayWorkerEnv): Promise<Response> {
    configureGateway(env, { requireCanisterId: true });
    const rpcResponse = await handleRpcHttp({
      method: request.method,
      origin: request.headers.get("origin") ?? undefined,
      readBodyText: () => readWorkerBody(request),
    });
    return new Response(rpcResponse.body, {
      status: rpcResponse.status,
      headers: rpcResponse.headers,
    });
  },
};

export async function readWorkerBody(request: Request): Promise<string> {
  const contentLength = request.headers.get("content-length");
  if (contentLength !== null) {
    const parsed = Number.parseInt(contentLength, 10);
    if (Number.isFinite(parsed) && parsed > CONFIG.maxHttpBodySize) {
      throw new Error("payload too large");
    }
  }
  const body = await request.arrayBuffer();
  if (body.byteLength > CONFIG.maxHttpBodySize) {
    throw new Error("payload too large");
  }
  return new TextDecoder().decode(body);
}
