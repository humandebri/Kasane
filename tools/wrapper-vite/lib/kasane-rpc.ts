// どこで: Kasane RPC helper / 何を: MetaMask unwrap に必要な JSON-RPC 参照と receipt 照会を提供 / なぜ: canister 認証に依存せず EVM 送信前後を扱うため

import type { EthereumProvider, MetaMaskChainConfig } from "@/lib/wallet/metamask";

type JsonRpcRequest = {
  jsonrpc: "2.0";
  id: number;
  method: string;
  params: readonly unknown[];
};

export type KasaneTransactionStatus = {
  transactionHash: string;
  transactionStatus: "Pending" | "Succeeded" | "Failed";
  blockNumber: string | null;
  explorerUrl: string | null;
  errorCode: string | null;
};

function normalizeHexQuantity(value: string, code: string): string {
  const trimmed = value.trim().toLowerCase();
  if (!/^0x[0-9a-f]+$/u.test(trimmed)) {
    throw new Error(code);
  }
  return trimmed;
}

function ensureString(value: unknown, code: string): string {
  if (typeof value !== "string") {
    throw new Error(code);
  }
  return value;
}

function buildExplorerUrl(baseUrl: string | null, transactionHash: string): string | null {
  if (baseUrl === null) {
    return null;
  }
  return `${baseUrl.replace(/\/$/u, "")}/tx/${transactionHash}`;
}

let jsonRpcRequestId = 0;

async function callKasaneRpc(
  rpcUrl: string,
  method: string,
  params: readonly unknown[],
): Promise<unknown> {
  jsonRpcRequestId += 1;
  const request: JsonRpcRequest = {
    jsonrpc: "2.0",
    id: jsonRpcRequestId,
    method,
    params,
  };
  const response = await fetch(rpcUrl, {
    method: "POST",
    headers: {
      "content-type": "application/json",
    },
    body: JSON.stringify(request),
  });
  if (!response.ok) {
    throw new Error(`kasane.rpc_http_failed:${response.status}`);
  }
  const parsed = await response.json();
  if (typeof parsed !== "object" || parsed === null) {
    throw new Error("kasane.rpc_response_invalid");
  }
  if (Reflect.has(parsed, "error")) {
    const error = Reflect.get(parsed, "error");
    if (typeof error !== "object" || error === null) {
      throw new Error("kasane.rpc_response_invalid");
    }
    const code = Reflect.get(error, "code");
    const message = Reflect.get(error, "message");
    if (typeof code !== "number" || typeof message !== "string") {
      throw new Error("kasane.rpc_response_invalid");
    }
    throw new Error(`kasane.rpc_failed:${code}:${message}`);
  }
  if (!Reflect.has(parsed, "result")) {
    throw new Error("kasane.rpc_response_invalid");
  }
  return Reflect.get(parsed, "result");
}

export function toRpcHex(value: bigint): string {
  return `0x${value.toString(16)}`;
}

export async function estimateMetaMaskUnwrapTransaction(args: {
  rpcUrl: string;
  from: string;
  to: string;
  data: string;
  valueWei?: bigint;
}): Promise<{
  nonce: string;
  gas: string;
  maxFeePerGas: string;
  maxPriorityFeePerGas: string;
}> {
  const [nonceRaw, gasRaw, gasPriceRaw, priorityFeeRaw] = await Promise.all([
    callKasaneRpc(args.rpcUrl, "eth_getTransactionCount", [args.from, "pending"]),
    callKasaneRpc(args.rpcUrl, "eth_estimateGas", [{
      from: args.from,
      to: args.to,
      data: args.data,
      value: toRpcHex(args.valueWei ?? 0n),
    }]),
    callKasaneRpc(args.rpcUrl, "eth_gasPrice", []),
    callKasaneRpc(args.rpcUrl, "eth_maxPriorityFeePerGas", []).catch(() => "0x0"),
  ]);
  return {
    nonce: normalizeHexQuantity(ensureString(nonceRaw, "kasane.rpc_nonce_invalid"), "kasane.rpc_nonce_invalid"),
    gas: normalizeHexQuantity(ensureString(gasRaw, "kasane.rpc_gas_invalid"), "kasane.rpc_gas_invalid"),
    maxFeePerGas: normalizeHexQuantity(
      ensureString(gasPriceRaw, "kasane.rpc_gas_price_invalid"),
      "kasane.rpc_gas_price_invalid",
    ),
    maxPriorityFeePerGas: normalizeHexQuantity(
      ensureString(priorityFeeRaw, "kasane.rpc_priority_fee_invalid"),
      "kasane.rpc_priority_fee_invalid",
    ),
  };
}

export async function sendMetaMaskTransaction(args: {
  provider: EthereumProvider;
  chainConfig: MetaMaskChainConfig;
  from: string;
  to: string;
  data: string;
  valueWei?: bigint;
  nonce: string;
  gas: string;
  maxFeePerGas: string;
  maxPriorityFeePerGas: string;
}): Promise<string> {
  const out = await args.provider.request({
    method: "eth_sendTransaction",
    params: [{
      from: args.from,
      to: args.to,
      data: args.data,
      value: toRpcHex(args.valueWei ?? 0n),
      nonce: args.nonce,
      gas: args.gas,
      maxFeePerGas: args.maxFeePerGas,
      maxPriorityFeePerGas: args.maxPriorityFeePerGas,
      chainId: toRpcHex(args.chainConfig.chainId),
    }],
  });
  return normalizeHexQuantity(
    ensureString(out, "kasane.metamask_tx_hash_invalid"),
    "kasane.metamask_tx_hash_invalid",
  );
}

export async function getKasaneTransactionStatus(args: {
  rpcUrl: string;
  transactionHash: string;
  explorerBaseUrl: string | null;
}): Promise<KasaneTransactionStatus> {
  const receipt = await callKasaneRpc(args.rpcUrl, "eth_getTransactionReceipt", [args.transactionHash]);
  if (receipt === null) {
    return {
      transactionHash: args.transactionHash,
      transactionStatus: "Pending",
      blockNumber: null,
      explorerUrl: buildExplorerUrl(args.explorerBaseUrl, args.transactionHash),
      errorCode: null,
    };
  }
  if (typeof receipt !== "object") {
    throw new Error("kasane.rpc_receipt_invalid");
  }
  const maybeStatus = Reflect.get(receipt, "status");
  const maybeBlockNumber = Reflect.get(receipt, "blockNumber");
  const status = typeof maybeStatus === "string" ? maybeStatus.toLowerCase() : "";
  const blockNumber = typeof maybeBlockNumber === "string" ? maybeBlockNumber : null;
  return {
    transactionHash: args.transactionHash,
    transactionStatus: status === "0x1" ? "Succeeded" : "Failed",
    blockNumber,
    explorerUrl: buildExplorerUrl(args.explorerBaseUrl, args.transactionHash),
    errorCode: status === "0x1" ? null : "kasane.tx_failed",
  };
}
