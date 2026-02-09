// どこで: JSON-RPCコア / 何を: request検証とerror整形 / なぜ: 形式エラーを仕様どおり返すため

export type JsonRpcId = string | number | null;

export type JsonRpcRequest = {
  jsonrpc: "2.0";
  id?: JsonRpcId;
  method: string;
  params?: unknown;
};

export type JsonRpcSuccess = {
  jsonrpc: "2.0";
  id: JsonRpcId;
  result: unknown;
};

export type JsonRpcErrorObject = {
  code: number;
  message: string;
  data?: unknown;
};

export type JsonRpcErrorResponse = {
  jsonrpc: "2.0";
  id: JsonRpcId;
  error: JsonRpcErrorObject;
};

export type JsonRpcResponse = JsonRpcSuccess | JsonRpcErrorResponse;

export function parseJsonWithDepthLimit(text: string, maxDepth: number): unknown {
  const value = JSON.parse(text) as unknown;
  const depth = computeDepth(value);
  if (depth > maxDepth) {
    throw new Error(`JSON depth exceeds limit: ${maxDepth}`);
  }
  return value;
}

export function computeDepth(value: unknown): number {
  if (value === null || typeof value !== "object") {
    return 1;
  }
  if (Array.isArray(value)) {
    let maxChild = 0;
    for (const item of value) {
      maxChild = Math.max(maxChild, computeDepth(item));
    }
    return maxChild + 1;
  }
  let maxChild = 0;
  for (const item of Object.values(value as Record<string, unknown>)) {
    maxChild = Math.max(maxChild, computeDepth(item));
  }
  return maxChild + 1;
}

export function validateRequest(value: unknown): JsonRpcRequest | null {
  if (!isRecord(value)) {
    return null;
  }
  if (value.jsonrpc !== "2.0") {
    return null;
  }
  if (typeof value.method !== "string" || value.method.length === 0) {
    return null;
  }
  if ("id" in value && !isValidId(value.id)) {
    return null;
  }
  return value as JsonRpcRequest;
}

export function isNotification(req: JsonRpcRequest): boolean {
  return !("id" in req);
}

function isValidId(value: unknown): value is JsonRpcId {
  return value === null || typeof value === "string" || typeof value === "number";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export const ERR_PARSE = -32700;
export const ERR_INVALID_REQUEST = -32600;
export const ERR_METHOD_NOT_FOUND = -32601;
export const ERR_INVALID_PARAMS = -32602;
export const ERR_INTERNAL = -32603;
export const ERR_METHOD_NOT_SUPPORTED = -32004;

export function makeError(id: JsonRpcId, code: number, message: string, data?: unknown): JsonRpcErrorResponse {
  return {
    jsonrpc: "2.0",
    id,
    error: data === undefined ? { code, message } : { code, message, data },
  };
}

export function makeSuccess(id: JsonRpcId, result: unknown): JsonRpcSuccess {
  return { jsonrpc: "2.0", id, result };
}
