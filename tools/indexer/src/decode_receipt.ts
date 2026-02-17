// どこで: receiptデコード / 何を: receipt payloadからstatus/erc20 transferを抽出 / なぜ: decode.tsの責務を分離して保守しやすくするため

export type ReceiptStatusInfo = {
  txHash: Buffer;
  status: 0 | 1;
};

export type Erc20TransferInfo = {
  txHash: Buffer;
  blockNumber: bigint;
  txIndex: number;
  logIndex: number;
  tokenAddress: Buffer;
  fromAddress: Buffer;
  toAddress: Buffer;
  amount: bigint;
};

export type DecodedReceiptsInfo = {
  statuses: ReceiptStatusInfo[];
  tokenTransfers: Erc20TransferInfo[];
  skippedTokenTransfers: number;
};

const HASH_LEN = 32;
const ADDRESS_LEN = 20;
const U128_LEN = 16;
const RECEIPT_V2_MAGIC = Buffer.from("7263707476320002", "hex");
const ERC20_TRANSFER_TOPIC0 = Buffer.from("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef", "hex");

export function decodeReceiptsPayload(payload: Uint8Array): DecodedReceiptsInfo {
  const data = Buffer.from(payload);
  const statuses: ReceiptStatusInfo[] = [];
  const tokenTransfers: Erc20TransferInfo[] = [];
  let skippedTokenTransfers = 0;
  let offset = 0;
  while (offset < data.length) {
    const remaining = data.length - offset;
    if (remaining < HASH_LEN + 4) {
      throw new Error("receipts payload truncated");
    }
    const txHash = Buffer.from(data.subarray(offset, offset + HASH_LEN));
    offset += HASH_LEN;
    const receiptLen = readU32Safe(data, offset, "receipts payload length header missing");
    offset += 4;
    if (data.length - offset < receiptLen) {
      throw new Error("receipts payload length mismatch");
    }
    const receipt = data.subarray(offset, offset + receiptLen);
    offset += receiptLen;

    const decoded = decodeReceipt(receipt);
    statuses.push({ txHash, status: decoded.status });
    skippedTokenTransfers += decoded.skippedTokenTransfers;

    for (const transfer of decoded.tokenTransfers) {
      tokenTransfers.push({
        txHash,
        blockNumber: decoded.blockNumber,
        txIndex: decoded.txIndex,
        logIndex: transfer.logIndex,
        tokenAddress: transfer.tokenAddress,
        fromAddress: transfer.fromAddress,
        toAddress: transfer.toAddress,
        amount: transfer.amount,
      });
    }
  }
  return { statuses, tokenTransfers, skippedTokenTransfers };
}

function decodeReceipt(encoded: Buffer): {
  blockNumber: bigint;
  txIndex: number;
  status: 0 | 1;
  tokenTransfers: Array<{
    logIndex: number;
    tokenAddress: Buffer;
    fromAddress: Buffer;
    toAddress: Buffer;
    amount: bigint;
  }>;
  skippedTokenTransfers: number;
} {
  let offset = 0;
  const isV2 =
    encoded.length >= RECEIPT_V2_MAGIC.length && encoded.subarray(0, RECEIPT_V2_MAGIC.length).equals(RECEIPT_V2_MAGIC);
  if (isV2) {
    offset += RECEIPT_V2_MAGIC.length;
  }

  readSlice(encoded, offset, HASH_LEN, "receipt tx_id missing");
  offset += HASH_LEN;

  const blockNumber = readU64Safe(encoded, offset, "receipt block_number missing");
  offset += 8;
  const txIndex = readU32Safe(encoded, offset, "receipt tx_index missing");
  offset += 4;
  const statusByte = readU8Safe(encoded, offset, "receipt status missing");
  offset += 1;
  if (statusByte !== 0 && statusByte !== 1) {
    throw new Error("receipt status must be 0 or 1");
  }

  offset += 8;
  offset += 8;
  if (isV2) {
    offset += U128_LEN * 3;
  }

  readSlice(encoded, offset, HASH_LEN, "receipt return_data_hash missing");
  offset += HASH_LEN;

  const returnDataLen = readU32Safe(encoded, offset, "receipt return_data_len missing");
  offset += 4;
  readSlice(encoded, offset, returnDataLen, "receipt return_data missing");
  offset += returnDataLen;

  readSlice(encoded, offset, 1, "receipt contract flag missing");
  offset += 1;
  readSlice(encoded, offset, ADDRESS_LEN, "receipt contract address bytes missing");
  offset += ADDRESS_LEN;

  const logsLen = readU32Safe(encoded, offset, "receipt logs_len missing");
  offset += 4;

  const tokenTransfers: Array<{
    logIndex: number;
    tokenAddress: Buffer;
    fromAddress: Buffer;
    toAddress: Buffer;
    amount: bigint;
  }> = [];
  let skippedTokenTransfers = 0;

  for (let logIndex = 0; logIndex < logsLen; logIndex += 1) {
    const tokenAddress = readSlice(encoded, offset, ADDRESS_LEN, "receipt log address missing");
    offset += ADDRESS_LEN;

    const topicsLen = readU32Safe(encoded, offset, "receipt log topics_len missing");
    offset += 4;
    const topics: Buffer[] = [];
    for (let i = 0; i < topicsLen; i += 1) {
      const topic = readSlice(encoded, offset, HASH_LEN, "receipt log topic missing");
      offset += HASH_LEN;
      topics.push(topic);
    }

    const dataLen = readU32Safe(encoded, offset, "receipt log data_len missing");
    offset += 4;
    const data = readSlice(encoded, offset, dataLen, "receipt log data missing");
    offset += dataLen;

    if (!isErc20TransferTopic(topics)) {
      continue;
    }
    if (topics.length < 3) {
      skippedTokenTransfers += 1;
      continue;
    }
    const topic1 = topics[1];
    const topic2 = topics[2];
    if (!topic1 || topic1.length !== HASH_LEN || !topic2 || topic2.length !== HASH_LEN) {
      skippedTokenTransfers += 1;
      continue;
    }
    if (data.length < HASH_LEN) {
      skippedTokenTransfers += 1;
      continue;
    }

    tokenTransfers.push({
      logIndex,
      tokenAddress,
      fromAddress: Buffer.from(topic1.subarray(12)),
      toAddress: Buffer.from(topic2.subarray(12)),
      // ERC-20 Transfer の amount は uint256 のため先頭32byteのみを読む。
      amount: bytesToBigInt(data.subarray(0, HASH_LEN)),
    });
  }

  return {
    blockNumber,
    txIndex,
    status: statusByte,
    tokenTransfers,
    skippedTokenTransfers,
  };
}

function isErc20TransferTopic(topics: Buffer[]): boolean {
  const topic0 = topics[0];
  return topic0 !== undefined && topic0.equals(ERC20_TRANSFER_TOPIC0);
}

function bytesToBigInt(bytes: Buffer): bigint {
  let out = 0n;
  for (const value of bytes) {
    out = (out << 8n) | BigInt(value);
  }
  return out;
}

function readSlice(data: Buffer, offset: number, len: number, err: string): Buffer {
  if (len < 0 || data.length - offset < len) {
    throw new Error(err);
  }
  return Buffer.from(data.subarray(offset, offset + len));
}

function readU64Safe(data: Buffer, offset: number, err: string): bigint {
  if (data.length - offset < 8) {
    throw new Error(err);
  }
  const high = data.readUInt32BE(offset);
  const low = data.readUInt32BE(offset + 4);
  return (BigInt(high) << 32n) + BigInt(low);
}

function readU32Safe(data: Buffer, offset: number, err: string): number {
  if (data.length - offset < 4) {
    throw new Error(err);
  }
  return data.readUInt32BE(offset);
}

function readU8Safe(data: Buffer, offset: number, err: string): number {
  if (data.length - offset < 1) {
    throw new Error(err);
  }
  return data.readUInt8(offset);
}
