// どこで: JSON-RPCハンドラ / 何を: methodごとの変換とcanister呼び出しを実装 / なぜ: Ethereum風インタフェースをGatewayで提供するため

import { CONFIG } from "./config";
import { getActor, type EthBlockView, type EthReceiptView, type EthTxView } from "./client";
import { bytesToQuantity, ensureLen, parseDataHex, parseQuantityHex, toDataHex, toQuantityHex } from "./hex";
import {
  ERR_INTERNAL,
  ERR_INVALID_PARAMS,
  ERR_METHOD_NOT_FOUND,
  ERR_METHOD_NOT_SUPPORTED,
  JsonRpcRequest,
  JsonRpcResponse,
  makeError,
  makeSuccess,
} from "./jsonrpc";

const ZERO_ADDR = "0x0000000000000000000000000000000000000000";
const ZERO_32 = `0x${"0".repeat(64)}`;
const ZERO_8 = `0x${"0".repeat(16)}`;
const ZERO_256 = `0x${"0".repeat(512)}`;

export async function handleRpc(req: JsonRpcRequest): Promise<JsonRpcResponse | null> {
  const id = req.id ?? null;
  try {
    switch (req.method) {
      case "web3_clientVersion":
        return makeSuccess(id, CONFIG.clientVersion);
      case "eth_syncing":
        return makeSuccess(id, false);
      case "eth_chainId": {
        const actor = await getActor();
        return makeSuccess(id, toQuantityHex(await actor.rpc_eth_chain_id()));
      }
      case "net_version": {
        const actor = await getActor();
        return makeSuccess(id, (await actor.rpc_eth_chain_id()).toString(10));
      }
      case "eth_blockNumber": {
        const actor = await getActor();
        return makeSuccess(id, toQuantityHex(await actor.rpc_eth_block_number()));
      }
      case "eth_getBlockByNumber":
        return await onGetBlockByNumber(id, req.params);
      case "eth_getTransactionByHash":
        return await onGetTransactionByHash(id, req.params);
      case "eth_getTransactionReceipt":
        return await onGetTransactionReceipt(id, req.params);
      case "eth_getBalance":
        return await onGetBalance(id, req.params);
      case "eth_getCode":
        return await onGetCode(id, req.params);
      case "eth_getStorageAt":
        return makeError(id, ERR_METHOD_NOT_FOUND, "eth_getStorageAt is not implemented on canister yet");
      case "eth_call":
        return await onEthCall(id, req.params);
      case "eth_estimateGas":
        return await onEstimateGas(id, req.params);
      case "eth_sendRawTransaction":
        return await onSendRawTransaction(id, req.params);
      default:
        return makeError(id, ERR_METHOD_NOT_FOUND, "method not found");
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return makeError(id, ERR_INTERNAL, "internal error", { detail: message });
  }
}

async function onGetBlockByNumber(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [blockTagRaw, fullTxRaw] = asParams(params, 2);
  const fullTx = typeof fullTxRaw === "boolean" ? fullTxRaw : false;
  const actor = await getActor();
  const number = await resolveBlockTag(blockTagRaw, actor.rpc_eth_block_number);
  const blockOpt = await actor.rpc_eth_get_block_by_number(number, fullTx);
  if (blockOpt.length === 0) {
    return makeSuccess(id, null);
  }
  return makeSuccess(id, mapBlock(blockOpt[0], fullTx));
}

async function onGetTransactionByHash(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [hashRaw] = asParams(params, 1);
  if (typeof hashRaw !== "string") {
    return makeError(id, ERR_INVALID_PARAMS, "tx hash must be hex string");
  }
  const txHash = ensureLen(parseDataHex(hashRaw), 32, "tx hash");
  const actor = await getActor();
  const txOpt = await actor.rpc_eth_get_transaction_by_eth_hash(txHash);
  if (txOpt.length === 0) {
    return makeSuccess(id, null);
  }
  return makeSuccess(id, mapTx(txOpt[0]));
}

async function onGetTransactionReceipt(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [hashRaw] = asParams(params, 1);
  if (typeof hashRaw !== "string") {
    return makeError(id, ERR_INVALID_PARAMS, "tx hash must be hex string");
  }
  const txHash = ensureLen(parseDataHex(hashRaw), 32, "tx hash");
  const actor = await getActor();
  const receiptOpt = await actor.rpc_eth_get_transaction_receipt_by_eth_hash(txHash);
  if (receiptOpt.length === 0) {
    return makeSuccess(id, null);
  }
  return makeSuccess(id, mapReceipt(receiptOpt[0], txHash));
}

async function onGetBalance(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [addressRaw, blockTagRaw] = asParams(params, 2);
  if (typeof addressRaw !== "string") {
    return makeError(id, ERR_INVALID_PARAMS, "address must be hex string");
  }
  if (!isLatestTag(blockTagRaw)) {
    return makeError(id, ERR_INVALID_PARAMS, "only latest blockTag is supported");
  }
  const address = ensureLen(parseDataHex(addressRaw), 20, "address");
  const actor = await getActor();
  const out = await actor.rpc_eth_get_balance(address);
  if ("Err" in out) {
    return makeError(id, -32000, "state unavailable", { detail: out.Err });
  }
  return makeSuccess(id, toQuantityHex(bytesToQuantity(out.Ok)));
}

async function onGetCode(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [addressRaw, blockTagRaw] = asParams(params, 2);
  if (typeof addressRaw !== "string") {
    return makeError(id, ERR_INVALID_PARAMS, "address must be hex string");
  }
  if (!isLatestTag(blockTagRaw)) {
    return makeError(id, ERR_INVALID_PARAMS, "only latest blockTag is supported");
  }
  const address = ensureLen(parseDataHex(addressRaw), 20, "address");
  const actor = await getActor();
  const out = await actor.rpc_eth_get_code(address);
  if ("Err" in out) {
    return makeError(id, -32000, "state unavailable", { detail: out.Err });
  }
  return makeSuccess(id, toDataHex(out.Ok));
}

async function onEthCall(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [callRaw, blockTagRaw] = asParams(params, 2);
  if (!isLatestTag(blockTagRaw)) {
    return makeError(id, ERR_INVALID_PARAMS, "only latest blockTag is supported");
  }
  const rawHex = getRawTxHexFromCallParam(callRaw);
  if (!rawHex) {
    return makeError(
      id,
      ERR_INVALID_PARAMS,
      "eth_call currently requires raw tx hex as params[0] or params[0].raw"
    );
  }
  const actor = await getActor();
  const out = await actor.rpc_eth_call_rawtx(parseDataHex(rawHex));
  if ("Err" in out) {
    return makeError(id, -32000, "execution reverted", { detail: out.Err });
  }
  return makeSuccess(id, toDataHex(out.Ok));
}

async function onEstimateGas(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  void params;
  return makeError(id, ERR_METHOD_NOT_SUPPORTED, "method not supported");
}

async function onSendRawTransaction(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [rawTxRaw] = asParams(params, 1);
  if (typeof rawTxRaw !== "string") {
    return makeError(id, ERR_INVALID_PARAMS, "raw tx must be hex string");
  }
  const actor = await getActor();
  const out = await actor.rpc_eth_send_raw_transaction(parseDataHex(rawTxRaw));
  if ("Err" in out) {
    const [code, detail] = mapSubmitError(out.Err);
    return makeError(id, code, "submit failed", detail);
  }
  return makeSuccess(id, toDataHex(out.Ok));
}

function mapSubmitError(err: { Internal: string } | { Rejected: string } | { InvalidArgument: string }): [number, unknown] {
  if ("InvalidArgument" in err) {
    return [-32602, { kind: "InvalidArgument", detail: err.InvalidArgument }];
  }
  if ("Rejected" in err) {
    return [-32000, { kind: "Rejected", detail: err.Rejected }];
  }
  return [-32603, { kind: "Internal", detail: err.Internal }];
}

function asParams(params: unknown, minLen: number): unknown[] {
  if (!Array.isArray(params)) {
    throw new Error("params must be array");
  }
  if (params.length < minLen) {
    throw new Error(`params must include at least ${minLen} entries`);
  }
  return params;
}

async function resolveBlockTag(blockTag: unknown, getHead: () => Promise<bigint>): Promise<bigint> {
  if (typeof blockTag === "string") {
    if (blockTag === "latest" || blockTag === "pending" || blockTag === "safe" || blockTag === "finalized") {
      return await getHead();
    }
    return parseQuantityHex(blockTag);
  }
  throw new Error("blockTag must be latest or QUANTITY");
}

function isLatestTag(blockTag: unknown): boolean {
  if (blockTag === undefined || blockTag === null) {
    return true;
  }
  return blockTag === "latest" || blockTag === "pending" || blockTag === "safe" || blockTag === "finalized";
}

function mapBlock(block: EthBlockView, fullTx: boolean): Record<string, unknown> {
  return {
    number: toQuantityHex(block.number),
    hash: toDataHex(block.block_hash),
    parentHash: toDataHex(block.parent_hash),
    nonce: ZERO_8,
    sha3Uncles: ZERO_32,
    logsBloom: ZERO_256,
    transactionsRoot: ZERO_32,
    stateRoot: toDataHex(block.state_root),
    receiptsRoot: ZERO_32,
    miner: ZERO_ADDR,
    difficulty: "0x0",
    totalDifficulty: "0x0",
    extraData: "0x",
    size: "0x0",
    gasLimit: "0x0",
    gasUsed: "0x0",
    timestamp: toQuantityHex(block.timestamp),
    transactions: mapBlockTxs(block.txs, fullTx),
    uncles: [],
    baseFeePerGas: "0x0",
  };
}

function mapBlockTxs(txs: { Full: EthTxView[] } | { Hashes: Uint8Array[] }, fullTx: boolean): unknown[] {
  if ("Hashes" in txs) {
    return txs.Hashes.map((v) => toDataHex(v));
  }
  if (!fullTx) {
    return txs.Full.map((v) => toDataHex(v.eth_tx_hash.length === 0 ? v.hash : v.eth_tx_hash[0]));
  }
  return txs.Full.map(mapTx);
}

function mapTx(tx: EthTxView): Record<string, unknown> {
  const decoded = tx.decoded.length === 0 ? null : tx.decoded[0];
  const txHash = tx.eth_tx_hash.length === 0 ? tx.hash : tx.eth_tx_hash[0];
  return {
    hash: toDataHex(txHash),
    nonce: decoded ? toQuantityHex(decoded.nonce) : "0x0",
    blockHash: null,
    blockNumber: tx.block_number.length === 0 ? null : toQuantityHex(tx.block_number[0]),
    transactionIndex: tx.tx_index.length === 0 ? null : toQuantityHex(BigInt(tx.tx_index[0])),
    from: decoded ? toDataHex(decoded.from) : ZERO_ADDR,
    to: decoded ? (decoded.to.length === 0 ? null : toDataHex(decoded.to[0])) : null,
    value: decoded ? toQuantityHex(bytesToQuantity(decoded.value)) : "0x0",
    gas: decoded ? toQuantityHex(decoded.gas_limit) : "0x0",
    gasPrice: decoded ? toQuantityHex(decoded.gas_price) : "0x0",
    input: decoded ? toDataHex(decoded.input) : "0x",
    type: "0x0",
    v: "0x0",
    r: "0x0",
    s: "0x0",
  };
}

function mapReceipt(receipt: EthReceiptView, fallbackTxHash: Uint8Array): Record<string, unknown> {
  const txHash = receipt.eth_tx_hash.length === 0 ? fallbackTxHash : receipt.eth_tx_hash[0];
  return {
    transactionHash: toDataHex(txHash),
    transactionIndex: toQuantityHex(BigInt(receipt.tx_index)),
    blockHash: null,
    blockNumber: toQuantityHex(receipt.block_number),
    from: ZERO_ADDR,
    to: null,
    cumulativeGasUsed: toQuantityHex(receipt.gas_used),
    gasUsed: toQuantityHex(receipt.gas_used),
    contractAddress: receipt.contract_address.length === 0 ? null : toDataHex(receipt.contract_address[0]),
    logs: [],
    logsBloom: ZERO_256,
    status: toQuantityHex(BigInt(receipt.status)),
    type: "0x0",
    effectiveGasPrice: toQuantityHex(receipt.effective_gas_price),
  };
}

function getRawTxHexFromCallParam(value: unknown): string | null {
  if (typeof value === "string") {
    return value;
  }
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const record = value as Record<string, unknown>;
  const raw = record.raw;
  return typeof raw === "string" ? raw : null;
}
