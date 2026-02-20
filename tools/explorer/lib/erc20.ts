// どこで: Explorer表示層のERC-20補助 / 何を: receipt logsからTransferイベントを抽出 / なぜ: tx詳細でトークン移動を見やすくするため

import { toHexLower } from "./hex";
import type { ReceiptView } from "./rpc";

const TOPIC_ERC20_TRANSFER = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

export type Erc20TransferView = {
  tokenAddressHex: string;
  fromAddressHex: string;
  toAddressHex: string;
  amount: bigint;
  logIndex: number;
};

export function extractErc20TransfersFromReceipt(receipt: ReceiptView): Erc20TransferView[] {
  const out: Erc20TransferView[] = [];
  for (let i = 0; i < receipt.logs.length; i += 1) {
    const log = receipt.logs[i];
    if (!log) {
      continue;
    }
    const topic0 = log.topics[0];
    if (!topic0 || toHexLower(topic0) !== TOPIC_ERC20_TRANSFER) {
      continue;
    }
    const fromTopic = log.topics[1];
    const toTopic = log.topics[2];
    if (!fromTopic || !toTopic || fromTopic.length !== 32 || toTopic.length !== 32 || log.data.length < 32) {
      continue;
    }
    out.push({
      tokenAddressHex: toHexLower(log.address),
      fromAddressHex: toHexLower(fromTopic.subarray(12)),
      toAddressHex: toHexLower(toTopic.subarray(12)),
      amount: readWord(log.data, 0),
      logIndex: i,
    });
  }
  return out;
}

function readWord(data: Uint8Array, wordIndex: number): bigint {
  const start = wordIndex * 32;
  const end = start + 32;
  if (data.length < end) {
    return 0n;
  }
  let out = 0n;
  for (let i = start; i < end; i += 1) {
    const value = data[i];
    if (value === undefined) {
      return 0n;
    }
    out = (out << 8n) + BigInt(value);
  }
  return out;
}
