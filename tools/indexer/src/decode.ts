// どこで: payloadデコード / 何を: block/tx_indexの最小デコード / なぜ: v2最小取り込みのため

export type BlockInfo = {
  number: bigint;
  blockHash: Buffer;
  timestamp: bigint;
  txIds: Buffer[];
};

export type TxIndexInfo = {
  txHash: Buffer;
  blockNumber: bigint;
  txIndex: number;
  callerPrincipal: Buffer | null;
  fromAddress: Buffer;
  toAddress: Buffer | null;
};

export type ReceiptStatusInfo = {
  txHash: Buffer;
  status: 0 | 1;
};

const HASH_LEN = 32;
const ADDRESS_LEN = 20;
const RECEIPT_V2_MAGIC = Buffer.from("7263707476320002", "hex");

export function decodeBlockPayload(payload: Uint8Array): BlockInfo {
  const data = Buffer.from(payload);
  // BlockData v2: number, parent_hash, block_hash, timestamp,
  // base_fee_per_gas, block_gas_limit, gas_used, tx_list_hash, state_root, tx_len
  const baseLen = 8 + HASH_LEN + HASH_LEN + 8 + 8 + 8 + 8 + HASH_LEN + HASH_LEN + 4;
  if (data.length < baseLen) {
    throw new Error("block payload too short");
  }
  // Rust側Storableと同じ順序で読み出す（big-endian固定）
  let offset = 0;
  const number = readU64BE(data, offset);
  offset += 8;
  offset += HASH_LEN; // parent_hash
  const blockHash = data.subarray(offset, offset + HASH_LEN);
  offset += HASH_LEN;
  const timestamp = readU64BE(data, offset);
  offset += 8;
  offset += 8; // base_fee_per_gas
  offset += 8; // block_gas_limit
  offset += 8; // gas_used
  offset += HASH_LEN; // tx_list_hash
  offset += HASH_LEN; // state_root
  const txLen = readU32BE(data, offset);
  offset += 4;
  const txCount = Number(txLen);
  const expected = baseLen + txCount * HASH_LEN;
  if (expected !== data.length) {
    throw new Error("block payload length mismatch");
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
    if (len !== expectedLen) {
      throw new Error("tx_index entry size mismatch: entry_len does not match to_len");
    }
    if (data.length - offset < toLen) {
      throw new Error("tx_index entry size mismatch: to address bytes missing");
    }
    const toAddress = toLen === 0 ? null : Buffer.from(data.subarray(offset, offset + toLen));
    offset += toLen;
    out.push({
      txHash: Buffer.from(txHash),
      blockNumber,
      txIndex,
      callerPrincipal,
      fromAddress,
      toAddress,
    });
  }
  return out;
}

export function decodeReceiptStatusPayload(payload: Uint8Array): ReceiptStatusInfo[] {
  const data = Buffer.from(payload);
  const out: ReceiptStatusInfo[] = [];
  let offset = 0;
  while (offset < data.length) {
    const remaining = data.length - offset;
    if (remaining < HASH_LEN + 4) {
      throw new Error("receipts payload truncated");
    }
    const txHash = Buffer.from(data.subarray(offset, offset + HASH_LEN));
    offset += HASH_LEN;
    const receiptLen = readU32BE(data, offset);
    offset += 4;
    if (data.length - offset < receiptLen) {
      throw new Error("receipts payload length mismatch");
    }
    const receipt = data.subarray(offset, offset + receiptLen);
    offset += receiptLen;
    const status = decodeReceiptStatus(receipt);
    out.push({ txHash, status });
  }
  return out;
}

function readU64BE(data: Buffer, offset: number): bigint {
  const high = data.readUInt32BE(offset);
  const low = data.readUInt32BE(offset + 4);
  return (BigInt(high) << 32n) + BigInt(low);
}

function readU32BE(data: Buffer, offset: number): number {
  return data.readUInt32BE(offset);
}

function decodeReceiptStatus(encoded: Buffer): 0 | 1 {
  let offset = 0;
  if (encoded.length >= RECEIPT_V2_MAGIC.length && encoded.subarray(0, RECEIPT_V2_MAGIC.length).equals(RECEIPT_V2_MAGIC)) {
    offset += RECEIPT_V2_MAGIC.length;
  }
  const minLen = offset + HASH_LEN + 8 + 4 + 1;
  if (encoded.length < minLen) {
    throw new Error("receipt bytes too short");
  }
  // tx_id(32) + block_number(8) + tx_index(4) の直後が status(1)
  const statusOffset = offset + HASH_LEN + 8 + 4;
  const status = encoded.readUInt8(statusOffset);
  if (status !== 0 && status !== 1) {
    throw new Error("receipt status must be 0 or 1");
  }
  return status;
}
