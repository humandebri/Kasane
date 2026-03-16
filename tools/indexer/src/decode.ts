// どこで: payloadデコード / 何を: block/tx_indexの最小デコードとreceiptデコード呼び出し / なぜ: workerの責務を単純化するため

import {
  decodeReceiptsPayload as decodeReceiptsPayloadImpl,
  type DecodedReceiptsInfo,
  type Erc20TransferInfo,
  type ReceiptStatusInfo,
} from "./decode_receipt";

export type BlockInfo = {
  number: bigint;
  blockHash: Buffer;
  timestamp: bigint;
  gasUsed: bigint;
  txIds: Buffer[];
};

export type TxIndexInfo = {
  txHash: Buffer;
  ethTxHash: Buffer | null;
  blockNumber: bigint;
  txIndex: number;
  callerPrincipal: Buffer | null;
  fromAddress: Buffer;
  toAddress: Buffer | null;
  txSelector: Buffer | null;
};

export type InternalTransactionInfo = {
  txHash: Buffer;
  blockNumber: bigint;
  txIndex: number;
  traceId: string;
  traceSortKey: string;
  depth: number;
  actionType:
    | "call"
    | "callcode"
    | "delegatecall"
    | "staticcall"
    | "create"
    | "create2"
    | "custom"
    | "selfdestruct";
  fromAddress: Buffer;
  toAddress: Buffer | null;
  value: bigint;
  createdContractAddress: Buffer | null;
  success: boolean;
  errorCode: string | null;
};

export type InternalTraceTxInfo = {
  txHash: Buffer;
  failed: boolean;
  truncated: boolean;
  capturedCount: number;
  totalCount: number;
};

export type DecodedInternalTracesInfo = {
  transactions: InternalTransactionInfo[];
  txs: InternalTraceTxInfo[];
};

export type { DecodedReceiptsInfo, Erc20TransferInfo, ReceiptStatusInfo };

const HASH_LEN = 32;
const ADDRESS_LEN = 20;

export function decodeBlockPayload(payload: Uint8Array): BlockInfo {
  const data = Buffer.from(payload);
  // v2: ... gas_used, beneficiary, tx_list_hash, state_root, tx_len
  // v1: ... gas_used, tx_list_hash, state_root, tx_len
  // mainnetには旧形式が混在するため、両方を順に試す。
  const parsedV2 = tryDecodeBlockPayload(data, true);
  if (parsedV2) {
    return parsedV2;
  }
  const parsedV1 = tryDecodeBlockPayload(data, false);
  if (parsedV1) {
    return parsedV1;
  }
  throw new Error("block payload length mismatch");
}

function tryDecodeBlockPayload(data: Buffer, hasBeneficiary: boolean): BlockInfo | null {
  const baseLen =
    8 + HASH_LEN + HASH_LEN + 8 + 8 + 8 + 8 + (hasBeneficiary ? ADDRESS_LEN : 0) + HASH_LEN + HASH_LEN + 4;
  if (data.length < baseLen) {
    return null;
  }
  let offset = 0;
  const number = readU64BE(data, offset);
  offset += 8;
  offset += HASH_LEN;
  const blockHash = data.subarray(offset, offset + HASH_LEN);
  offset += HASH_LEN;
  const timestamp = readU64BE(data, offset);
  offset += 8;
  offset += 8;
  offset += 8;
  const gasUsed = readU64BE(data, offset);
  offset += 8;
  if (hasBeneficiary) {
    offset += ADDRESS_LEN;
  }
  offset += HASH_LEN;
  offset += HASH_LEN;
  const txLen = readU32BE(data, offset);
  offset += 4;
  const txCount = Number(txLen);
  const expected = baseLen + txCount * HASH_LEN;
  if (expected !== data.length) {
    return null;
  }
  const txIds: Buffer[] = [];
  for (let i = 0; i < txCount; i += 1) {
    const tx = data.subarray(offset, offset + HASH_LEN);
    txIds.push(tx);
    offset += HASH_LEN;
  }
  return {
    number,
    blockHash: Buffer.from(blockHash),
    timestamp,
    gasUsed,
    txIds,
  };
}

export function decodeTxIndexPayload(payload: Uint8Array): TxIndexInfo[] {
  const data = Buffer.from(payload);
  const out: TxIndexInfo[] = [];
  let offset = 0;
  while (offset < data.length) {
    const remaining = data.length - offset;
    if (remaining < HASH_LEN + 4) {
      throw new Error("tx_index payload truncated");
    }
    const txHash = data.subarray(offset, offset + HASH_LEN);
    offset += HASH_LEN;
    const len = readU32BE(data, offset);
    offset += 4;
    if (data.length - offset < len) {
      throw new Error("tx_index payload length mismatch");
    }
    if (len < 35) {
      throw new Error("tx_index entry size mismatch: entry must include 35+ bytes (u64 + u32 + principal_len + from + to_len)");
    }
    const blockNumber = readU64BE(data, offset);
    offset += 8;
    const txIndex = readU32BE(data, offset);
    offset += 4;
    const principalLen = data.readUInt16BE(offset);
    offset += 2;
    if (data.length - offset < principalLen + ADDRESS_LEN + 1) {
      throw new Error("tx_index entry size mismatch: missing from/to fields");
    }
    const expectedMinLen = 12 + 2 + principalLen + ADDRESS_LEN + 1;
    if (len < expectedMinLen) {
      throw new Error("tx_index entry size mismatch: entry_len is smaller than required fields");
    }
    const expectedLenBase = expectedMinLen;
    const principalEnd = offset + principalLen;
    let callerPrincipal: Buffer | null = null;
    if (principalLen > 0) {
      callerPrincipal = Buffer.from(data.subarray(offset, offset + principalLen));
      offset += principalLen;
    }
    if (offset !== principalEnd) {
      throw new Error("tx_index entry size mismatch: principal length does not match");
    }
    const fromAddress = Buffer.from(data.subarray(offset, offset + ADDRESS_LEN));
    offset += ADDRESS_LEN;
    const toLen = data.readUInt8(offset);
    offset += 1;
    if (toLen !== 0 && toLen !== ADDRESS_LEN) {
      throw new Error("tx_index entry size mismatch: to_len must be 0 or 20");
    }
    const expectedLen = expectedLenBase + toLen;
    if (len < expectedLen + 1) {
      throw new Error("tx_index entry size mismatch: entry_len does not match to_len");
    }
    if (data.length - offset < toLen) {
      throw new Error("tx_index entry size mismatch: to address bytes missing");
    }
    const toAddress = toLen === 0 ? null : Buffer.from(data.subarray(offset, offset + toLen));
    offset += toLen;
    const extraLen = len - expectedLen;
    const selectorLen = data.readUInt8(offset);
    offset += 1;
    if (selectorLen !== 0 && selectorLen !== 4) {
      throw new Error("tx_index entry size mismatch: selector_len must be 0 or 4");
    }
    if (extraLen < 1 + selectorLen + 1) {
      throw new Error("tx_index entry size mismatch: selector/eth hash length mismatch");
    }
    let txSelector: Buffer | null = null;
    if (selectorLen > 0) {
      if (data.length - offset < selectorLen) {
        throw new Error("tx_index entry size mismatch: selector bytes missing");
      }
      txSelector = Buffer.from(data.subarray(offset, offset + selectorLen));
      offset += selectorLen;
    }
    if (data.length - offset < 1) {
      throw new Error("tx_index entry size mismatch: missing eth_hash_len");
    }
    const ethHashLen = data.readUInt8(offset);
    offset += 1;
    if (ethHashLen !== 0 && ethHashLen !== HASH_LEN) {
      throw new Error("tx_index entry size mismatch: eth_hash_len must be 0 or 32");
    }
    if (extraLen !== 1 + selectorLen + 1 + ethHashLen) {
      throw new Error("tx_index entry size mismatch: entry_len does not match eth_hash_len");
    }
    if (data.length - offset < ethHashLen) {
      throw new Error("tx_index entry size mismatch: eth hash bytes missing");
    }
    const ethTxHash = ethHashLen === 0 ? null : Buffer.from(data.subarray(offset, offset + ethHashLen));
    offset += ethHashLen;
    out.push({
      txHash: Buffer.from(txHash),
      ethTxHash,
      blockNumber,
      txIndex,
      callerPrincipal,
      fromAddress,
      toAddress,
      txSelector,
    });
  }
  return out;
}

export function decodeReceiptsPayload(payload: Uint8Array): DecodedReceiptsInfo {
  return decodeReceiptsPayloadImpl(payload);
}

export function decodeInternalTracesPayload(payload: Uint8Array): DecodedInternalTracesInfo {
  const data = Buffer.from(payload);
  const transactions: InternalTransactionInfo[] = [];
  const txs: InternalTraceTxInfo[] = [];
  let offset = 0;
  while (offset < data.length) {
    if (data.length - offset < HASH_LEN + 4) {
      throw new Error("internal_traces payload truncated");
    }
    const txHash = Buffer.from(data.subarray(offset, offset + HASH_LEN));
    offset += HASH_LEN;
    const entryLen = readU32BE(data, offset);
    offset += 4;
    if (data.length - offset < entryLen) {
      throw new Error("internal_traces payload length mismatch");
    }
    const end = offset + entryLen;
    const version = readU8Within(data, offset, end);
    offset += 1;
    if (version !== 2 && version !== 3) {
      throw new Error("internal_traces unsupported version");
    }
    let failed = false;
    if (version >= 3) {
      const failedByte = readU8Within(data, offset, end);
      offset += 1;
      if (failedByte !== 0 && failedByte !== 1) {
        throw new Error("internal_traces invalid failed flag");
      }
      failed = failedByte === 1;
    }
    const truncatedByte = readU8Within(data, offset, end);
    offset += 1;
    if (truncatedByte !== 0 && truncatedByte !== 1) {
      throw new Error("internal_traces invalid truncated flag");
    }
    const capturedCount = readU32BEWithin(data, offset, end);
    offset += 4;
    const totalCount = readU32BEWithin(data, offset, end);
    offset += 4;
    const count = readU32BEWithin(data, offset, end);
    offset += 4;
    if (capturedCount !== count) {
      throw new Error("internal_traces captured_count mismatch");
    }
    if (totalCount < capturedCount) {
      throw new Error("internal_traces total_count mismatch");
    }
    txs.push({
      txHash,
      failed,
      truncated: truncatedByte === 1,
      capturedCount,
      totalCount,
    });
    for (let i = 0; i < count; i += 1) {
      const blockNumber = readU64BEWithin(data, offset, end);
      offset += 8;
      const txIndex = readU32BEWithin(data, offset, end);
      offset += 4;
      const traceIdLen = readU16BEWithin(data, offset, end);
      offset += 2;
      ensureRemaining(end, offset, traceIdLen, "internal_traces trace_id truncated");
      if (data.length - offset < traceIdLen) {
        throw new Error("internal_traces trace_id truncated");
      }
      const traceId = data.subarray(offset, offset + traceIdLen).toString("utf8");
      offset += traceIdLen;
      const depth = readU16BEWithin(data, offset, end);
      offset += 2;
      const actionType = decodeInternalActionType(readU8Within(data, offset, end));
      offset += 1;
      ensureRemaining(end, offset, ADDRESS_LEN + 1 + 32 + 1 + 1 + 2, "internal_traces entry truncated");
      if (data.length - offset < ADDRESS_LEN + 1 + 32 + 1 + 1 + 2) {
        throw new Error("internal_traces entry truncated");
      }
      const fromAddress = Buffer.from(data.subarray(offset, offset + ADDRESS_LEN));
      offset += ADDRESS_LEN;
      const toAddressFlag = readU8Within(data, offset, end);
      offset += 1;
      let toAddress: Buffer | null = null;
      if (toAddressFlag === 1) {
        ensureRemaining(end, offset, ADDRESS_LEN, "internal_traces to_address truncated");
        toAddress = Buffer.from(data.subarray(offset, offset + ADDRESS_LEN));
        offset += ADDRESS_LEN;
      } else if (toAddressFlag !== 0) {
        throw new Error("internal_traces invalid to flag");
      }
      ensureRemaining(end, offset, 32, "internal_traces value truncated");
      const value = readU256BE(data.subarray(offset, offset + 32));
      offset += 32;
      const createdFlag = readU8Within(data, offset, end);
      offset += 1;
      let createdContractAddress: Buffer | null = null;
      if (createdFlag === 1) {
        ensureRemaining(end, offset, ADDRESS_LEN, "internal_traces created address truncated");
        createdContractAddress = Buffer.from(data.subarray(offset, offset + ADDRESS_LEN));
        offset += ADDRESS_LEN;
      } else if (createdFlag !== 0) {
        throw new Error("internal_traces invalid created flag");
      }
      const successByte = readU8Within(data, offset, end);
      offset += 1;
      if (successByte !== 0 && successByte !== 1) {
        throw new Error("internal_traces invalid success flag");
      }
      const errorLen = readU16BEWithin(data, offset, end);
      offset += 2;
      ensureRemaining(end, offset, errorLen, "internal_traces error_code truncated");
      if (data.length - offset < errorLen) {
        throw new Error("internal_traces error_code truncated");
      }
      const errorCode = errorLen === 0 ? null : data.subarray(offset, offset + errorLen).toString("utf8");
      offset += errorLen;
      transactions.push({
        txHash,
        blockNumber,
        txIndex,
        traceId,
        traceSortKey: traceIdToSortKey(traceId),
        depth,
        actionType,
        fromAddress,
        toAddress,
        value,
        createdContractAddress,
        success: successByte === 1,
        errorCode,
      });
    }
  if (offset !== end) {
      throw new Error("internal_traces entry framing mismatch");
    }
  }
  return { transactions, txs };
}

export function decodeReceiptStatusPayload(payload: Uint8Array): ReceiptStatusInfo[] {
  return decodeReceiptsPayloadImpl(payload).statuses;
}

export function decodeErc20TransferPayload(payload: Uint8Array): Erc20TransferInfo[] {
  return decodeReceiptsPayloadImpl(payload).tokenTransfers;
}

function readU64BE(data: Buffer, offset: number): bigint {
  const high = data.readUInt32BE(offset);
  const low = data.readUInt32BE(offset + 4);
  return (BigInt(high) << 32n) + BigInt(low);
}

function readU32BE(data: Buffer, offset: number): number {
  return data.readUInt32BE(offset);
}

function traceIdToSortKey(traceId: string): string {
  if (!/^[0-9]+(?:_[0-9]+)*$/.test(traceId)) {
    throw new Error("internal_traces invalid trace_id");
  }
  return traceId
    .split("_")
    .map((segment) => segment.padStart(10, "0"))
    .join("_");
}

function ensureRemaining(end: number, offset: number, len: number, message: string): void {
  if (end - offset < len) {
    throw new Error(message);
  }
}

function readU8Within(data: Buffer, offset: number, end: number): number {
  ensureRemaining(end, offset, 1, "internal_traces entry truncated");
  return data.readUInt8(offset);
}

function readU16BEWithin(data: Buffer, offset: number, end: number): number {
  ensureRemaining(end, offset, 2, "internal_traces entry truncated");
  return data.readUInt16BE(offset);
}

function readU32BEWithin(data: Buffer, offset: number, end: number): number {
  ensureRemaining(end, offset, 4, "internal_traces entry truncated");
  return readU32BE(data, offset);
}

function readU64BEWithin(data: Buffer, offset: number, end: number): bigint {
  ensureRemaining(end, offset, 8, "internal_traces entry truncated");
  return readU64BE(data, offset);
}

function readU256BE(data: Buffer): bigint {
  if (data.length !== 32) {
    throw new Error("u256 length mismatch");
  }
  return BigInt(`0x${data.toString("hex")}`);
}

function decodeInternalActionType(
  actionRaw: number
): InternalTransactionInfo["actionType"] {
  switch (actionRaw) {
    case 1:
      return "call";
    case 2:
      return "callcode";
    case 3:
      return "delegatecall";
    case 4:
      return "staticcall";
    case 5:
      return "create";
    case 6:
      return "create2";
    case 7:
      return "custom";
    case 8:
      return "selfdestruct";
    default:
      throw new Error("internal_traces invalid action type");
  }
}
