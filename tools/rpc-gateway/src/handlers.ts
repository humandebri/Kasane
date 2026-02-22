// どこで: JSON-RPCハンドラ / 何を: methodごとの変換とcanister呼び出しを実装 / なぜ: Ethereum風インタフェースをGatewayで提供するため
import { CONFIG } from "./config";
import {
  type EthLogFilterView,
  type EthLogsCursorView,
  type EthLogsPageView,
  getActor,
  type CallObject,
  type EthBlockView,
  type EthReceiptView,
  type EthTxView,
  type OpsStatusView,
} from "./client";
import { bytesToQuantity, ensureLen, parseDataHex, parseQuantityHex, toDataHex, toQuantityHex } from "./hex";
import { ERR_INTERNAL, ERR_INVALID_PARAMS, ERR_METHOD_NOT_FOUND, JsonRpcRequest, JsonRpcResponse, makeError, makeSuccess } from "./jsonrpc";
const ZERO_ADDR = "0x0000000000000000000000000000000000000000";
const ZERO_32 = `0x${"0".repeat(64)}`;
const ZERO_8 = `0x${"0".repeat(16)}`;
const ZERO_256 = `0x${"0".repeat(512)}`;
const LOGS_PAGE_LIMIT = 500;
const LOGS_MAX_PAGES = 20;
const SUPPORTED_CALL_KEYS = new Set([
  "to",
  "from",
  "gas",
  "gasPrice",
  "value",
  "data",
  "nonce",
  "maxFeePerGas",
  "maxPriorityFeePerGas",
  "accessList",
  "chainId",
  "type",
]);
type ParsedAccessListItem = { address: string; storageKeys: string[] };
type ParsedCallObject = {
  to?: string;
  from?: string;
  gas?: string;
  gasPrice?: string;
  value?: string;
  data?: string;
  nonce?: string;
  maxFeePerGas?: string;
  maxPriorityFeePerGas?: string;
  accessList?: ParsedAccessListItem[];
  chainId?: string;
  type?: string;
};
type ParsedLogsFilter = {
  candid: EthLogFilterView;
};

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
      case "eth_gasPrice":
        return await onGasPrice(id);
      case "eth_getBlockByNumber":
        return await onGetBlockByNumber(id, req.params);
      case "eth_getTransactionByHash":
        return await onGetTransactionByHash(id, req.params);
      case "eth_getTransactionReceipt":
        return await onGetTransactionReceipt(id, req.params);
      case "eth_getBalance":
        return await onGetBalance(id, req.params);
      case "eth_getTransactionCount":
        return await onGetTransactionCount(id, req.params);
      case "eth_getCode":
        return await onGetCode(id, req.params);
      case "eth_getStorageAt":
        return await onGetStorageAt(id, req.params);
      case "eth_getLogs":
        return await onGetLogs(id, req.params);
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
  let number: bigint;
  try {
    number = await resolveBlockTag(blockTagRaw, actor.rpc_eth_block_number);
  } catch (error) {
    return makeInvalidParams(id, error);
  }
  const blockLookup = await actor.rpc_eth_get_block_by_number_with_status(number, fullTx);
  if ("NotFound" in blockLookup) {
    return makeSuccess(id, null);
  }
  if ("Pruned" in blockLookup) {
    return makeError(id, -32001, "resource not found", {
      reason: "block.pruned",
      pruned_before_block: toQuantityHex(blockLookup.Pruned.pruned_before_block),
    });
  }
  const mapped = mapBlock(blockLookup.Found, fullTx);
  if ("error" in mapped) {
    return makeError(id, -32000, "legacy block metadata unavailable", { detail: mapped.error });
  }
  return makeSuccess(id, mapped.value);
}

async function onGasPrice(id: string | number | null): Promise<JsonRpcResponse> {
  const actor = await getActor();
  const head = await actor.rpc_eth_block_number();
  const blockLookup = await actor.rpc_eth_get_block_by_number_with_status(head, false);
  if (!("Found" in blockLookup)) {
    return makeError(id, -32000, "state unavailable", { detail: "latest block is unavailable" });
  }
  if (blockLookup.Found.base_fee_per_gas.length === 0) {
    return makeError(id, -32000, "state unavailable", { detail: "base_fee_per_gas is unavailable" });
  }
  return makeSuccess(id, toQuantityHex(blockLookup.Found.base_fee_per_gas[0]));
}

async function onGetTransactionByHash(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [hashRaw] = asParams(params, 1);
  if (typeof hashRaw !== "string") {
    return makeError(id, ERR_INVALID_PARAMS, "tx hash must be hex string");
  }
  let txHash: Uint8Array;
  try {
    txHash = ensureLen(parseDataHex(hashRaw), 32, "tx hash");
  } catch (error) {
    return makeInvalidParams(id, error);
  }
  const actor = await getActor();
  const readinessError = txHashReadinessError(id, await actor.get_ops_status());
  if (readinessError !== null) {
    return readinessError;
  }
  const txOpt = await actor.rpc_eth_get_transaction_by_eth_hash(txHash);
  return makeSuccess(id, txOpt.length === 0 ? null : mapTx(txOpt[0]));
}

async function onGetTransactionReceipt(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [hashRaw] = asParams(params, 1);
  if (typeof hashRaw !== "string") {
    return makeError(id, ERR_INVALID_PARAMS, "tx hash must be hex string");
  }
  let txHash: Uint8Array;
  try {
    txHash = ensureLen(parseDataHex(hashRaw), 32, "tx hash");
  } catch (error) {
    return makeInvalidParams(id, error);
  }
  const actor = await getActor();
  const readinessError = txHashReadinessError(id, await actor.get_ops_status());
  if (readinessError !== null) {
    return readinessError;
  }
  const receiptLookup = await actor.rpc_eth_get_transaction_receipt_with_status(txHash);
  if ("NotFound" in receiptLookup) {
    return makeSuccess(id, null);
  }
  if ("PossiblyPruned" in receiptLookup) {
    return makeError(id, -32001, "resource not found", {
      reason: "receipt.possibly_pruned",
      pruned_before_block: toQuantityHex(receiptLookup.PossiblyPruned.pruned_before_block),
    });
  }
  if ("Pruned" in receiptLookup) {
    return makeError(id, -32001, "resource not found", {
      reason: "receipt.pruned",
      pruned_before_block: toQuantityHex(receiptLookup.Pruned.pruned_before_block),
    });
  }
  return makeSuccess(id, mapReceipt(receiptLookup.Found, txHash));
}

function txHashReadinessError(id: string | number | null, status: OpsStatusView): JsonRpcResponse | null {
  if (status.needs_migration) {
    return makeError(id, -32000, "state unavailable", {
      reason: "ops.read.needs_migration",
      schema_version: status.schema_version,
    });
  }
  if (status.critical_corrupt) {
    return makeError(id, -32000, "state unavailable", {
      reason: "ops.read.critical_corrupt",
      schema_version: status.schema_version,
    });
  }
  return null;
}

async function onGetBalance(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [addressRaw, blockTagRaw] = asParams(params, 2);
  if (typeof addressRaw !== "string") {
    return makeError(id, ERR_INVALID_PARAMS, "address must be hex string");
  }
  if (!isLatestTag(blockTagRaw)) {
    return makeError(id, ERR_INVALID_PARAMS, "only latest blockTag is supported");
  }
  let address: Uint8Array;
  try {
    address = ensureLen(parseDataHex(addressRaw), 20, "address");
  } catch (error) {
    return makeInvalidParams(id, error);
  }
  const actor = await getActor();
  const out = await actor.rpc_eth_get_balance(address);
  return "Err" in out
    ? makeError(id, -32000, "state unavailable", { detail: out.Err })
    : makeSuccess(id, toQuantityHex(bytesToQuantity(out.Ok)));
}

async function onGetTransactionCount(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [addressRaw, blockTagRaw] = asTxCountParams(params);
  if (typeof addressRaw !== "string") {
    return makeError(id, ERR_INVALID_PARAMS, "address must be hex string");
  }
  if (!isLatestTag(blockTagRaw)) {
    return makeError(id, ERR_INVALID_PARAMS, "only latest/pending/safe/finalized blockTag is supported");
  }
  let address: Uint8Array;
  try {
    address = ensureLen(parseDataHex(addressRaw), 20, "address");
  } catch (error) {
    return makeInvalidParams(id, error);
  }
  const actor = await getActor();
  const out = await actor.expected_nonce_by_address(address);
  return "Err" in out
    ? makeError(id, -32000, "state unavailable", { detail: out.Err })
    : makeSuccess(id, toQuantityHex(out.Ok));
}

async function onGetCode(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [addressRaw, blockTagRaw] = asParams(params, 2);
  if (typeof addressRaw !== "string") {
    return makeError(id, ERR_INVALID_PARAMS, "address must be hex string");
  }
  if (!isLatestTag(blockTagRaw)) {
    return makeError(id, ERR_INVALID_PARAMS, "only latest blockTag is supported");
  }
  let address: Uint8Array;
  try {
    address = ensureLen(parseDataHex(addressRaw), 20, "address");
  } catch (error) {
    return makeInvalidParams(id, error);
  }
  const actor = await getActor();
  const out = await actor.rpc_eth_get_code(address);
  return "Err" in out
    ? makeError(id, -32000, "state unavailable", { detail: out.Err })
    : makeSuccess(id, toDataHex(out.Ok));
}

async function onGetStorageAt(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [addressRaw, slotRaw, blockTagRaw] = asParams(params, 3);
  if (typeof addressRaw !== "string" || typeof slotRaw !== "string") {
    return makeError(id, ERR_INVALID_PARAMS, "address/slot must be hex string");
  }
  if (!isLatestTag(blockTagRaw)) {
    return makeError(id, ERR_INVALID_PARAMS, "only latest blockTag is supported");
  }
  let address: Uint8Array;
  let slot: Uint8Array;
  try {
    address = ensureLen(parseDataHex(addressRaw), 20, "address");
    slot = normalizeStorageSlot32(slotRaw);
  } catch (error) {
    return makeInvalidParams(id, error);
  }
  const actor = await getActor();
  const out = await actor.rpc_eth_get_storage_at(address, slot);
  return "Err" in out
    ? makeError(id, -32000, "state unavailable", { detail: out.Err })
    : makeSuccess(id, toDataHex(out.Ok));
}

async function onGetLogs(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [filterRaw] = asParams(params, 1);
  const actor = await getActor();
  const parsed = await parseLogsFilter(filterRaw, actor.rpc_eth_block_number);
  if ("error" in parsed) {
    return makeError(id, ERR_INVALID_PARAMS, parsed.error);
  }
  const logs = await collectLogs(actor, parsed.value);
  if ("error" in logs) {
    return makeError(id, logs.error.code, logs.error.message, logs.error.data);
  }
  return makeSuccess(id, logs.value.map(mapLogItem));
}

async function onEthCall(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [callRaw, blockTagRaw] = asCallParams(params);
  if (!isLatestTag(blockTagRaw)) {
    return makeError(id, ERR_INVALID_PARAMS, "only latest blockTag is supported");
  }
  const call = parseCallObject(callRaw);
  if ("error" in call) {
    return makeError(id, ERR_INVALID_PARAMS, call.error);
  }
  let candidCall: CallObject;
  try {
    candidCall = toCandidCallObject(call);
  } catch (error) {
    return makeInvalidParams(id, error);
  }
  const actor = await getActor();
  const out = await actor.rpc_eth_call_object(candidCall);
  if ("Err" in out) {
    const code = classifyCallObjectErrCode(out.Err.code);
    return code === ERR_INVALID_PARAMS
      ? makeError(id, code, "invalid params", { detail: out.Err.message, rpc_code: out.Err.code })
      : makeError(id, code, "execution failed", { detail: out.Err.message, rpc_code: out.Err.code });
  }
  if (out.Ok.status === 0) {
    return makeError(id, -32000, "execution reverted", revertDataToHex(out.Ok.revert_data));
  }
  return makeSuccess(id, toDataHex(out.Ok.return_data));
}

async function onEstimateGas(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [callRaw, blockTagRaw] = asCallParams(params);
  if (!isLatestTag(blockTagRaw)) {
    return makeError(id, ERR_INVALID_PARAMS, "only latest blockTag is supported");
  }
  const call = parseCallObject(callRaw);
  if ("error" in call) {
    return makeError(id, ERR_INVALID_PARAMS, call.error);
  }
  let candidCall: CallObject;
  try {
    candidCall = toCandidCallObject(call);
  } catch (error) {
    return makeInvalidParams(id, error);
  }
  const actor = await getActor();
  const out = await actor.rpc_eth_estimate_gas_object(candidCall);
  const errCode = "Err" in out ? classifyCallObjectErrCode(out.Err.code) : -32000;
  return "Err" in out
    ? makeError(
        id,
        errCode,
        errCode === ERR_INVALID_PARAMS ? "invalid params" : "estimate failed",
        { detail: out.Err.message, rpc_code: out.Err.code }
      )
    : makeSuccess(id, toQuantityHex(out.Ok));
}

async function onSendRawTransaction(id: string | number | null, params: unknown): Promise<JsonRpcResponse> {
  const [rawTxRaw] = asParams(params, 1);
  if (typeof rawTxRaw !== "string") {
    return makeError(id, ERR_INVALID_PARAMS, "raw tx must be hex string");
  }
  let rawTx: Uint8Array;
  try {
    rawTx = parseDataHex(rawTxRaw);
  } catch (error) {
    return makeInvalidParams(id, error);
  }
  const actor = await getActor();
  const out = await actor.rpc_eth_send_raw_transaction(rawTx);
  if ("Err" in out) {
    const mapped = mapSubmitError(out.Err);
    return makeError(id, mapped.code, "submit failed", mapped.data);
  }
  const resolved = await resolveSubmittedEthHash(actor, out.Ok);
  if (!resolved.ok) {
    return makeError(id, -32000, "submit succeeded but eth hash is unavailable", {
      reason: resolved.reason,
      tx_id: toDataHex(out.Ok),
    });
  }
  return makeSuccess(id, toDataHex(resolved.hash));
}

function mapSubmitError(err: { Internal: string } | { Rejected: string } | { InvalidArgument: string }): { code: number; data: unknown } {
  if ("InvalidArgument" in err) {
    return { code: -32602, data: { kind: "InvalidArgument", detail: err.InvalidArgument } };
  }
  if ("Rejected" in err) {
    return { code: -32000, data: { kind: "Rejected", detail: err.Rejected } };
  }
  return { code: -32603, data: { kind: "Internal", detail: err.Internal } };
}

async function resolveSubmittedEthHash(
  actor: Awaited<ReturnType<typeof getActor>>,
  txId: Uint8Array
): Promise<{ ok: true; hash: Uint8Array } | { ok: false; reason: string }> {
  const txOpt = await actor.rpc_eth_get_transaction_by_tx_id(txId);
  return resolveSubmittedEthHashFromLookup(txOpt);
}

function resolveSubmittedEthHashFromLookup(
  txOpt: [] | [EthTxView]
): { ok: true; hash: Uint8Array } | { ok: false; reason: string } {
  if (txOpt.length === 0) {
    return { ok: false, reason: "tx_id_not_found" };
  }
  const tx = txOpt[0];
  if ("EthSigned" in tx.kind && tx.eth_tx_hash.length === 0) {
    return { ok: false, reason: "eth_signed_missing_eth_tx_hash" };
  }
  return { ok: true, hash: tx.eth_tx_hash.length === 0 ? tx.hash : tx.eth_tx_hash[0] };
}

export function __test_resolve_submitted_eth_hash_from_lookup(
  txOpt: [] | [EthTxView]
): { ok: true; hash: Uint8Array } | { ok: false; reason: string } {
  return resolveSubmittedEthHashFromLookup(txOpt);
}

export function __test_tx_hash_readiness_error(
  id: string | number | null,
  status: OpsStatusView
): JsonRpcResponse | null {
  return txHashReadinessError(id, status);
}

export function __test_as_call_params(params: unknown): [unknown, unknown] {
  return asCallParams(params);
}

export function __test_as_tx_count_params(params: unknown): [unknown, unknown] {
  return asTxCountParams(params);
}

export async function __test_parse_logs_filter(
  filterRaw: unknown,
  head: bigint
): Promise<{ value: ParsedLogsFilter } | { error: string }> {
  return parseLogsFilter(filterRaw, async () => head);
}

export function __test_map_get_logs_error(
  err: { TooManyResults: null } | { RangeTooLarge: null } | { InvalidArgument: string } | { UnsupportedFilter: string }
): {
  code: number;
  message: string;
  data: unknown;
} {
  return mapGetLogsError(err);
}

function makeInvalidParams(id: string | number | null, error: unknown): JsonRpcResponse {
  return makeError(id, ERR_INVALID_PARAMS, toErrorMessage(error));
}

function toErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

function classifyCallObjectErrCode(code: number): number {
  if (code >= 1000 && code < 2000) {
    return ERR_INVALID_PARAMS;
  }
  return -32000;
}

function asParams(params: unknown, minLen: number): unknown[] {
  if (!Array.isArray(params) || params.length < minLen) {
    throw new Error(`params must include at least ${minLen} entries`);
  }
  return params;
}

function asCallParams(params: unknown): [unknown, unknown] {
  if (!Array.isArray(params) || params.length < 1) {
    throw new Error("params must include at least 1 entries");
  }
  const callRaw = params[0];
  const blockTagRaw = params.length >= 2 ? params[1] : "latest";
  return [callRaw, blockTagRaw];
}

function asTxCountParams(params: unknown): [unknown, unknown] {
  if (!Array.isArray(params) || params.length < 1) {
    throw new Error("params must include at least 1 entries");
  }
  const addressRaw = params[0];
  const blockTagRaw = params.length >= 2 ? params[1] : "latest";
  return [addressRaw, blockTagRaw];
}

async function parseLogsFilter(
  filterRaw: unknown,
  getHead: () => Promise<bigint>
): Promise<{ value: ParsedLogsFilter } | { error: string }> {
  if (!isRecord(filterRaw)) {
    return { error: "filter must be object" };
  }
  const supported = new Set(["fromBlock", "toBlock", "address", "topics", "blockHash"]);
  for (const key of Object.keys(filterRaw)) {
    if (!supported.has(key)) {
      return { error: `${key} is not a supported filter field` };
    }
  }
  if ("blockHash" in filterRaw && filterRaw.blockHash !== undefined) {
    return { error: "blockHash filter is not supported" };
  }
  if ("address" in filterRaw && filterRaw.address !== undefined) {
    if (typeof filterRaw.address !== "string") {
      return { error: "address must be hex string" };
    }
  }
  const fromBlock = await resolveLogsBlockTag(filterRaw.fromBlock, getHead);
  if ("error" in fromBlock) {
    return fromBlock;
  }
  const toBlock = await resolveLogsBlockTag(filterRaw.toBlock, getHead);
  if ("error" in toBlock) {
    return toBlock;
  }
  if (fromBlock.value !== undefined && toBlock.value !== undefined && fromBlock.value > toBlock.value) {
    return { error: "fromBlock must be <= toBlock" };
  }
  const topicsOut = parseTopicsFilter(filterRaw.topics);
  if ("error" in topicsOut) {
    return topicsOut;
  }
  let address: [] | [Uint8Array] = [];
  if (typeof filterRaw.address === "string") {
    try {
      address = [ensureLen(parseDataHex(filterRaw.address), 20, "address")];
    } catch (error) {
      return { error: toErrorMessage(error) };
    }
  }
  return {
    value: {
      candid: {
        limit: [],
        topic0: topicsOut.value.topic0,
        topic1: topicsOut.value.topic1,
        address,
        from_block: fromBlock.value === undefined ? [] : [fromBlock.value],
        to_block: toBlock.value === undefined ? [] : [toBlock.value],
      },
    },
  };
}

async function resolveLogsBlockTag(
  blockTag: unknown,
  getHead: () => Promise<bigint>
): Promise<{ value: bigint | undefined } | { error: string }> {
  if (blockTag === undefined || blockTag === null) {
    return { value: undefined };
  }
  if (typeof blockTag !== "string") {
    return { error: "blockTag must be latest/earliest/pending/safe/finalized or QUANTITY" };
  }
  if (blockTag === "earliest") {
    return { value: 0n };
  }
  if (isLatestTag(blockTag)) {
    return { value: await getHead() };
  }
  try {
    return { value: parseQuantityHex(blockTag) };
  } catch {
    return { error: "blockTag must be latest/earliest/pending/safe/finalized or QUANTITY" };
  }
}

function parseTopicsFilter(
  topicsRaw: unknown
): { value: { topic0: [] | [Uint8Array]; topic1: [] | [Uint8Array] } } | { error: string } {
  if (topicsRaw === undefined) {
    return { value: { topic0: [], topic1: [] } };
  }
  if (!Array.isArray(topicsRaw)) {
    return { error: "topics must be array" };
  }
  if (topicsRaw.length > 2) {
    for (let i = 2; i < topicsRaw.length; i += 1) {
      if (topicsRaw[i] !== null) {
        return { error: "only topics[0] and topics[1] are supported" };
      }
    }
  }
  const topic0 = parseTopicAt(topicsRaw[0], 0);
  if ("error" in topic0) {
    return topic0;
  }
  const topic1 = parseTopicAt(topicsRaw[1], 1);
  if ("error" in topic1) {
    return topic1;
  }
  if (topic1.value.length > 0) {
    return { error: "only topics[0] is supported" };
  }
  return { value: { topic0: topic0.value, topic1: topic1.value } };
}

function parseTopicAt(value: unknown, index: number): { value: [] | [Uint8Array] } | { error: string } {
  if (value === undefined || value === null) {
    return { value: [] };
  }
  if (Array.isArray(value)) {
    return { error: `topics[${index}] OR条件(array)は未対応です` };
  }
  if (typeof value !== "string") {
    return { error: `topics[${index}] must be hex string or null` };
  }
  try {
    return { value: [ensureLen(parseDataHex(value), 32, `topics[${index}]`)] };
  } catch (error) {
    return { error: toErrorMessage(error) };
  }
}

async function collectLogs(
  actor: Awaited<ReturnType<typeof getActor>>,
  filter: ParsedLogsFilter
): Promise<{ value: EthLogsPageView["items"] } | { error: { code: number; message: string; data: unknown } }> {
  let cursor: [] | [EthLogsCursorView] = [];
  let pages = 0;
  const items: EthLogsPageView["items"] = [];
  while (pages < LOGS_MAX_PAGES) {
    const page = await actor.rpc_eth_get_logs_paged(filter.candid, cursor, LOGS_PAGE_LIMIT);
    if ("Err" in page) {
      return { error: mapGetLogsError(page.Err) };
    }
    items.push(...page.Ok.items);
    if (page.Ok.next_cursor.length === 0) {
      return { value: items };
    }
    cursor = [page.Ok.next_cursor[0]];
    pages += 1;
  }
  return {
    error: {
      code: -32005,
      message: "limit exceeded",
      data: { detail: "logs pagination exceeded gateway safety limit", max_pages: LOGS_MAX_PAGES },
    },
  };
}

function mapGetLogsError(err: { TooManyResults: null } | { RangeTooLarge: null } | { InvalidArgument: string } | { UnsupportedFilter: string }): {
  code: number;
  message: string;
  data: unknown;
} {
  if ("InvalidArgument" in err) {
    return { code: ERR_INVALID_PARAMS, message: "invalid params", data: { detail: err.InvalidArgument } };
  }
  if ("UnsupportedFilter" in err) {
    return { code: ERR_INVALID_PARAMS, message: "invalid params", data: { detail: err.UnsupportedFilter } };
  }
  if ("RangeTooLarge" in err) {
    return { code: -32005, message: "limit exceeded", data: { reason: "logs.range_too_large" } };
  }
  return { code: -32005, message: "limit exceeded", data: { reason: "logs.too_many_results" } };
}

function mapLogItem(item: EthLogsPageView["items"][number]): Record<string, unknown> {
  const txHash = item.eth_tx_hash.length === 0 ? item.tx_hash : item.eth_tx_hash[0];
  return {
    address: toDataHex(item.address),
    topics: item.topics.map((topic) => toDataHex(topic)),
    data: toDataHex(item.data),
    blockNumber: toQuantityHex(item.block_number),
    blockHash: null,
    transactionHash: toDataHex(txHash),
    transactionIndex: toQuantityHex(BigInt(item.tx_index)),
    logIndex: toQuantityHex(BigInt(item.log_index)),
    removed: false,
  };
}

async function resolveBlockTag(blockTag: unknown, getHead: () => Promise<bigint>): Promise<bigint> {
  if (typeof blockTag !== "string") {
    throw new Error("blockTag must be latest or QUANTITY");
  }
  if (isLatestTag(blockTag)) {
    return await getHead();
  }
  return parseQuantityHex(blockTag);
}

function isLatestTag(blockTag: unknown): boolean {
  return blockTag === undefined || blockTag === null || blockTag === "latest" || blockTag === "pending" || blockTag === "safe" || blockTag === "finalized";
}

function parseCallObject(value: unknown): ParsedCallObject | { error: string } {
  if (!isRecord(value)) {
    return { error: "callObject must be object" };
  }
  for (const key of Object.keys(value)) {
    if (!SUPPORTED_CALL_KEYS.has(key)) {
      return { error: `${key} is not a supported callObject field` };
    }
  }
  const parsed: ParsedCallObject = {};
  if ("to" in value && value.to !== undefined && typeof value.to !== "string") {
    return { error: "to must be hex string" };
  }
  if ("from" in value && value.from !== undefined && typeof value.from !== "string") {
    return { error: "from must be hex string" };
  }
  if ("gas" in value && value.gas !== undefined && typeof value.gas !== "string") {
    return { error: "gas must be QUANTITY hex string" };
  }
  if ("gasPrice" in value && value.gasPrice !== undefined && typeof value.gasPrice !== "string") {
    return { error: "gasPrice must be QUANTITY hex string" };
  }
  if ("value" in value && value.value !== undefined && typeof value.value !== "string") {
    return { error: "value must be QUANTITY hex string" };
  }
  if ("data" in value && value.data !== undefined && typeof value.data !== "string") {
    return { error: "data must be DATA hex string" };
  }
  if ("nonce" in value && value.nonce !== undefined && typeof value.nonce !== "string") {
    return { error: "nonce must be QUANTITY hex string" };
  }
  if ("maxFeePerGas" in value && value.maxFeePerGas !== undefined && typeof value.maxFeePerGas !== "string") {
    return { error: "maxFeePerGas must be QUANTITY hex string" };
  }
  if ("maxPriorityFeePerGas" in value && value.maxPriorityFeePerGas !== undefined && typeof value.maxPriorityFeePerGas !== "string") {
    return { error: "maxPriorityFeePerGas must be QUANTITY hex string" };
  }
  if ("chainId" in value && value.chainId !== undefined && typeof value.chainId !== "string") {
    return { error: "chainId must be QUANTITY hex string" };
  }
  if ("type" in value && value.type !== undefined && typeof value.type !== "string") {
    return { error: "type must be QUANTITY hex string" };
  }
  if ("accessList" in value && value.accessList !== undefined) {
    const parsedAccessList = parseAccessList(value.accessList);
    if ("error" in parsedAccessList) {
      return parsedAccessList;
    }
    parsed.accessList = parsedAccessList;
  }
  if (typeof value.to === "string") parsed.to = value.to;
  if (typeof value.from === "string") parsed.from = value.from;
  if (typeof value.gas === "string") parsed.gas = value.gas;
  if (typeof value.gasPrice === "string") parsed.gasPrice = value.gasPrice;
  if (typeof value.value === "string") parsed.value = value.value;
  if (typeof value.data === "string") parsed.data = value.data;
  if (typeof value.nonce === "string") parsed.nonce = value.nonce;
  if (typeof value.maxFeePerGas === "string") parsed.maxFeePerGas = value.maxFeePerGas;
  if (typeof value.maxPriorityFeePerGas === "string") parsed.maxPriorityFeePerGas = value.maxPriorityFeePerGas;
  if (typeof value.chainId === "string") parsed.chainId = value.chainId;
  if (typeof value.type === "string") parsed.type = value.type;
  if (parsed.gasPrice !== undefined && (parsed.maxFeePerGas !== undefined || parsed.maxPriorityFeePerGas !== undefined)) {
    return { error: "gasPrice and maxFeePerGas/maxPriorityFeePerGas cannot be used together" };
  }
  if (parsed.maxPriorityFeePerGas !== undefined && parsed.maxFeePerGas === undefined) {
    return { error: "maxPriorityFeePerGas requires maxFeePerGas" };
  }
  if (parsed.maxPriorityFeePerGas !== undefined && parsed.maxFeePerGas !== undefined) {
    const maxPriority = parseQuantityHexSafe(parsed.maxPriorityFeePerGas, "maxPriorityFeePerGas");
    if ("error" in maxPriority) {
      return maxPriority;
    }
    const maxFee = parseQuantityHexSafe(parsed.maxFeePerGas, "maxFeePerGas");
    if ("error" in maxFee) {
      return maxFee;
    }
    if (maxPriority.value > maxFee.value) {
      return { error: "maxPriorityFeePerGas must be <= maxFeePerGas" };
    }
  }
  if (parsed.type !== undefined) {
    const txTypeOut = parseQuantityHexSafe(parsed.type, "type");
    if ("error" in txTypeOut) {
      return txTypeOut;
    }
    const txType = txTypeOut.value;
    if (txType !== 0n && txType !== 2n) {
      return { error: "type must be 0x0 or 0x2" };
    }
    if (txType === 0n && (parsed.maxFeePerGas !== undefined || parsed.maxPriorityFeePerGas !== undefined)) {
      return { error: "type=0 cannot be used with maxFeePerGas/maxPriorityFeePerGas" };
    }
    if (txType === 2n && parsed.gasPrice !== undefined) {
      return { error: "type=2 cannot be used with gasPrice" };
    }
  }
  return parsed;
}

function toCandidCallObject(call: ParsedCallObject): CallObject {
  return {
    to: call.to === undefined ? [] : [ensureLen(parseDataHex(call.to), 20, "to")],
    from: call.from === undefined ? [] : [ensureLen(parseDataHex(call.from), 20, "from")],
    gas: call.gas === undefined ? [] : [parseQuantityHex(call.gas)],
    gas_price: call.gasPrice === undefined ? [] : [parseQuantityHex(call.gasPrice)],
    nonce: call.nonce === undefined ? [] : [parseQuantityHex(call.nonce)],
    max_fee_per_gas: call.maxFeePerGas === undefined ? [] : [parseQuantityHex(call.maxFeePerGas)],
    max_priority_fee_per_gas:
      call.maxPriorityFeePerGas === undefined ? [] : [parseQuantityHex(call.maxPriorityFeePerGas)],
    chain_id: call.chainId === undefined ? [] : [parseQuantityHex(call.chainId)],
    tx_type: call.type === undefined ? [] : [parseQuantityHex(call.type)],
    access_list:
      call.accessList === undefined
        ? []
        : [
            call.accessList.map((item) => ({
              address: ensureLen(parseDataHex(item.address), 20, "accessList.address"),
              storage_keys: item.storageKeys.map((key) =>
                ensureLen(parseDataHex(key), 32, "accessList.storageKeys[]")
              ),
            })),
          ],
    value: call.value === undefined ? [] : [quantityToWord32(parseQuantityHex(call.value))],
    data: call.data === undefined ? [] : [parseDataHex(call.data)],
  };
}

export function __test_parse_call_object(value: unknown): ParsedCallObject | { error: string } {
  return parseCallObject(value);
}

export function __test_to_candid_call_object(call: ParsedCallObject): CallObject {
  return toCandidCallObject(call);
}

export function __test_normalize_storage_slot32(slot: string): Uint8Array {
  return normalizeStorageSlot32(slot);
}

export function __test_revert_data_hex(revertData: [] | [Uint8Array]): string {
  return revertDataToHex(revertData);
}

export function __test_classify_call_object_err_code(code: number): number {
  return classifyCallObjectErrCode(code);
}

function normalizeStorageSlot32(slot: string): Uint8Array {
  if (slot.startsWith("0x") && slot.length === 66) {
    return ensureLen(parseDataHex(slot), 32, "slot");
  }
  const quantity = parseQuantityHex(slot);
  const hex = quantity.toString(16);
  if (hex.length > 64) {
    throw new Error("slot must fit in 32 bytes");
  }
  return Uint8Array.from(Buffer.from(hex.padStart(64, "0"), "hex"));
}

function revertDataToHex(revertData: [] | [Uint8Array]): string {
  if (revertData.length === 0) {
    return "0x";
  }
  return toDataHex(revertData[0]);
}

function quantityToWord32(value: bigint): Uint8Array {
  if (value < 0n) {
    throw new Error("value must be non-negative");
  }
  const hex = value.toString(16).padStart(64, "0");
  if (hex.length > 64) {
    throw new Error("value must fit in 32 bytes");
  }
  return Uint8Array.from(Buffer.from(hex, "hex"));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  return true;
}

function parseAccessList(value: unknown): ParsedAccessListItem[] | { error: string } {
  if (!Array.isArray(value)) {
    return { error: "accessList must be an array" };
  }
  const out: ParsedAccessListItem[] = [];
  for (const item of value) {
    if (!isRecord(item)) {
      return { error: "accessList[] must be object" };
    }
    if (typeof item.address !== "string") {
      return { error: "accessList[].address must be hex string" };
    }
    if (!Array.isArray(item.storageKeys)) {
      return { error: "accessList[].storageKeys must be array" };
    }
    const storageKeys: string[] = [];
    for (const key of item.storageKeys) {
      if (typeof key !== "string") {
        return { error: "accessList[].storageKeys[] must be hex string" };
      }
      storageKeys.push(key);
    }
    out.push({ address: item.address, storageKeys });
  }
  return out;
}

function parseQuantityHexSafe(value: string, label: string): { value: bigint } | { error: string } {
  try {
    return { value: parseQuantityHex(value) };
  } catch {
    return { error: `${label} must be QUANTITY hex string` };
  }
}

function mapBlock(
  block: EthBlockView,
  fullTx: boolean
): { value: Record<string, unknown> } | { error: string } {
  if (block.base_fee_per_gas.length === 0 || block.gas_limit.length === 0 || block.gas_used.length === 0) {
    return { error: "missing base_fee_per_gas/gas_limit/gas_used in block payload" };
  }
  return {
    value: {
      number: toQuantityHex(block.number),
      hash: toDataHex(block.block_hash),
      parentHash: toDataHex(block.parent_hash),
      nonce: ZERO_8,
      sha3Uncles: ZERO_32,
      logsBloom: ZERO_256,
      transactionsRoot: ZERO_32,
      stateRoot: toDataHex(block.state_root),
      receiptsRoot: ZERO_32,
      miner: toDataHex(block.beneficiary),
      difficulty: "0x0",
      totalDifficulty: "0x0",
      extraData: "0x",
      size: "0x0",
      gasLimit: toQuantityHex(block.gas_limit[0]),
      gasUsed: toQuantityHex(block.gas_used[0]),
      timestamp: toQuantityHex(block.timestamp),
      transactions: mapBlockTxs(block.txs, fullTx),
      uncles: [],
      baseFeePerGas: toQuantityHex(block.base_fee_per_gas[0]),
    },
  };
}
function mapBlockTxs(txs: { Full: EthTxView[] } | { Hashes: Uint8Array[] }, fullTx: boolean): unknown[] { if ("Hashes" in txs) return txs.Hashes.map((v) => toDataHex(v)); return fullTx ? txs.Full.map(mapTx) : txs.Full.map((v) => toDataHex(v.eth_tx_hash.length === 0 ? v.hash : v.eth_tx_hash[0])); }
function mapTx(tx: EthTxView): Record<string, unknown> {
  const decoded = tx.decoded.length === 0 ? null : tx.decoded[0];
  const txHash = tx.eth_tx_hash.length === 0 ? tx.hash : tx.eth_tx_hash[0];
  const toAddr = decoded && decoded.to.length > 0 ? decoded.to[0] : undefined;
  const gasPrice = decoded?.gas_price[0] ?? decoded?.max_fee_per_gas[0];
  const maxFeePerGas = decoded?.max_fee_per_gas[0];
  const maxPriorityFeePerGas = decoded?.max_priority_fee_per_gas[0];
  return {
    hash: toDataHex(txHash),
    nonce: decoded ? toQuantityHex(decoded.nonce) : "0x0",
    blockHash: null,
    blockNumber: tx.block_number.length === 0 ? null : toQuantityHex(tx.block_number[0]),
    transactionIndex: tx.tx_index.length === 0 ? null : toQuantityHex(BigInt(tx.tx_index[0])),
    from: decoded ? toDataHex(decoded.from) : ZERO_ADDR,
    to: toAddr ? toDataHex(toAddr) : null,
    value: decoded ? toQuantityHex(bytesToQuantity(decoded.value)) : "0x0",
    gas: decoded ? toQuantityHex(decoded.gas_limit) : "0x0",
    gasPrice: gasPrice === undefined ? "0x0" : toQuantityHex(gasPrice),
    maxFeePerGas: maxFeePerGas === undefined ? null : toQuantityHex(maxFeePerGas),
    maxPriorityFeePerGas: maxPriorityFeePerGas === undefined ? null : toQuantityHex(maxPriorityFeePerGas),
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
    logs: receipt.logs.map((log) => ({
      address: toDataHex(log.address),
      topics: log.topics.map((topic) => toDataHex(topic)),
      data: toDataHex(log.data),
      blockNumber: toQuantityHex(receipt.block_number),
      blockHash: null,
      transactionHash: toDataHex(txHash),
      transactionIndex: toQuantityHex(BigInt(receipt.tx_index)),
      logIndex: toQuantityHex(BigInt(log.log_index)),
      removed: false,
    })),
    logsBloom: ZERO_256,
    status: toQuantityHex(BigInt(receipt.status)),
    type: "0x0",
    effectiveGasPrice: toQuantityHex(receipt.effective_gas_price),
  };
}

export function __test_map_receipt(receipt: EthReceiptView, fallbackTxHash: Uint8Array): Record<string, unknown> {
  return mapReceipt(receipt, fallbackTxHash);
}

export function __test_map_block(block: EthBlockView, fullTx: boolean): { value: Record<string, unknown> } | { error: string } {
  return mapBlock(block, fullTx);
}

export function __test_map_tx(tx: EthTxView): Record<string, unknown> {
  return mapTx(tx);
}
