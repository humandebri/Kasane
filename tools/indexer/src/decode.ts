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
};

const HASH_LEN = 32;

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
    if (len < 14) {
      throw new Error("tx_index entry size mismatch: entry must include 14+ bytes (u64 + u32 + principal_len)");
    }
    const blockNumber = readU64BE(data, offset);
    offset += 8;
    const txIndex = readU32BE(data, offset);
    offset += 4;
    const principalLen = data.readUInt16BE(offset);
    offset += 2;
    const expectedLen = 12 + 2 + principalLen;
    if (len !== expectedLen) {
      throw new Error("tx_index entry size mismatch: principal length does not match");
    }
    let callerPrincipal: Buffer | null = null;
    if (principalLen > 0) {
      callerPrincipal = Buffer.from(data.subarray(offset, offset + principalLen));
      offset += principalLen;
    }
    out.push({
      txHash: Buffer.from(txHash),
      blockNumber,
      txIndex,
      callerPrincipal,
    });
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
