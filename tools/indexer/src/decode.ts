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
