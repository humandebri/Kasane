// どこで: Receiptタイムライン再構成層 / 何を: logsからDeFiイベントを時系列に再構成 / なぜ: 複雑Txの可読性を上げるため

import { toHexLower } from "./hex";
import type { ReceiptView } from "./rpc";
type TimelineProtocol = "aave" | "uniswap_v2" | "uniswap_v3" | "erc20" | "unknown";
export type TimelineStepType =
  | "flash_borrow"
  | "swap"
  | "repay_candidate"
  | "transfer"
  | "approval"
  | "unknown";
export type TimelineStep = {
  index: number;
  type: TimelineStepType;
  protocol: TimelineProtocol;
  addressHex: string;
  topic0Hex: string | null;
  summary: string;
  raw: { topicsHex: string[]; dataHex: string };
};
export type TimelineView = {
  steps: TimelineStep[];
  counters: { borrow: number; swap: number; repay: number; transfer: number; unknown: number };
  notes: string[];
};
const TOPIC_ERC20_TRANSFER = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
const TOPIC_ERC20_APPROVAL = "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925";
const TOPIC_UNIV2_SWAP = "0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822";
const TOPIC_UNIV2_MINT = "0x4c209b5fc8ad50758f13e2e1088ba56a560dff690a1c6fef26394f4c03821c4f";
const TOPIC_UNIV2_BURN = "0xdccd412f0b1252819cb1fd330b93224ca42612892bb3f4f789976e6d81936496";
const TOPIC_UNIV3_SWAP = "0xc42079f94a6350d7e6235f29174924f928cc2ac818eb64fed8004e115fbcca67";
const TOPIC_AAVE_V2_FLASHLOAN = "0x631042c832b07452973831137f2d73e395028b44b250dedc5abb0ee766e168ac";
const TOPIC_AAVE_V3_FLASHLOAN = "0xefefaba5e921573100900a3ad9cf29f222d995fb3b6045797eaea7521bd8d6f0";
const TOPIC_AAVE_V3_FLASHLOAN_SIMPLE = "0xf164a7d9b7e450d8229718aed20376118864bcc756709e0fc1d0891133dd2fe8";

export function buildTimelineFromReceiptLogs(receipt: ReceiptView): TimelineView {
  const borrowContext = collectFlashBorrowContext(receipt);
  const steps: TimelineStep[] = [];
  for (let i = 0; i < receipt.logs.length; i += 1) {
    const log = receipt.logs[i];
    if (!log) {
      continue;
    }
    const parsed = parseLog(log, i, borrowContext);
    steps.push(parsed);
  }
  const counters = { borrow: 0, swap: 0, repay: 0, transfer: 0, unknown: 0 };
  for (const step of steps) {
    if (step.type === "flash_borrow") counters.borrow += 1;
    if (step.type === "swap") counters.swap += 1;
    if (step.type === "repay_candidate") counters.repay += 1;
    if (step.type === "transfer") counters.transfer += 1;
    if (step.type === "unknown") counters.unknown += 1;
  }
  return {
    steps,
    counters,
    notes: [
      "This view is reconstructed from logs, not an internal call trace.",
      "repay_candidate is a logs-based inference.",
      "Internal processing without emitted events is not shown.",
    ],
  };
}

type BorrowContext = {
  poolCandidates: Set<string>;
  firstBorrowIndexByPoolAsset: Map<string, number>;
};

function collectFlashBorrowContext(receipt: ReceiptView): BorrowContext {
  const poolCandidates = new Set<string>();
  const firstBorrowIndexByPoolAsset = new Map<string, number>();
  for (let i = 0; i < receipt.logs.length; i += 1) {
    const log = receipt.logs[i];
    if (!log) {
      continue;
    }
    const topic0 = log.topics[0];
    const topic0Hex = topic0 ? toHexLower(topic0) : null;
    if (topic0Hex === TOPIC_AAVE_V2_FLASHLOAN || topic0Hex === TOPIC_AAVE_V3_FLASHLOAN || topic0Hex === TOPIC_AAVE_V3_FLASHLOAN_SIMPLE) {
      const poolHex = toHexLower(log.address);
      poolCandidates.add(poolHex);
      try {
        const decoded = decodeAaveFlashLoanFields(log, topic0Hex);
        const key = `${poolHex}:${decoded.asset}`;
        const previous = firstBorrowIndexByPoolAsset.get(key);
        if (previous === undefined || i < previous) {
          firstBorrowIndexByPoolAsset.set(key, i);
        }
      } catch {
        // decode失敗時はpool候補だけ保持して継続する
      }
    }
  }
  return { poolCandidates, firstBorrowIndexByPoolAsset };
}

function parseLog(
  log: ReceiptView["logs"][number],
  index: number,
  borrowContext: BorrowContext
): TimelineStep {
  const topic0 = log.topics[0];
  const topic0Hex = topic0 ? toHexLower(topic0) : null;
  const addressHex = toHexLower(log.address);
  const raw = {
    topicsHex: log.topics.map((topic) => toHexLower(topic)),
    dataHex: toHexLower(log.data),
  };

  if (!topic0Hex) {
    return unknownStep(index, addressHex, topic0Hex, raw, "topic0 is missing");
  }
  try {
    if (topic0Hex === TOPIC_AAVE_V2_FLASHLOAN || topic0Hex === TOPIC_AAVE_V3_FLASHLOAN || topic0Hex === TOPIC_AAVE_V3_FLASHLOAN_SIMPLE) {
      const details = decodeAaveFlashLoan(log, topic0Hex);
      return { index, type: "flash_borrow", protocol: "aave", addressHex, topic0Hex, summary: details, raw };
    }

    if (topic0Hex === TOPIC_UNIV2_SWAP || topic0Hex === TOPIC_UNIV2_MINT || topic0Hex === TOPIC_UNIV2_BURN) {
      return { index, type: "swap", protocol: "uniswap_v2", addressHex, topic0Hex, summary: decodeUniswapV2(log, topic0Hex), raw };
    }

    if (topic0Hex === TOPIC_UNIV3_SWAP) {
      return { index, type: "swap", protocol: "uniswap_v3", addressHex, topic0Hex, summary: decodeUniswapV3Swap(log), raw };
    }

    if (topic0Hex === TOPIC_ERC20_TRANSFER) {
      const transfer = decodeErc20Transfer(log);
      const key = `${transfer.to}:${addressHex}`;
      const firstBorrowIndex = borrowContext.firstBorrowIndexByPoolAsset.get(key);
      const transferType: TimelineStepType =
        borrowContext.poolCandidates.has(transfer.to) &&
        firstBorrowIndex !== undefined &&
        firstBorrowIndex < index
          ? "repay_candidate"
          : "transfer";
      return {
        index, type: transferType, protocol: transferType === "repay_candidate" ? "aave" : "erc20", addressHex, topic0Hex,
        summary: transferType === "repay_candidate"
          ? `repay candidate token=${addressHex} from=${transfer.from} to=${transfer.to} amount=${transfer.amount.toString()}`
          : `transfer token=${addressHex} from=${transfer.from} to=${transfer.to} amount=${transfer.amount.toString()}`,
        raw,
      };
    }

    if (topic0Hex === TOPIC_ERC20_APPROVAL) {
      const approval = decodeErc20Approval(log);
      return { index, type: "approval", protocol: "erc20", addressHex, topic0Hex, summary: `approval token=${addressHex} owner=${approval.owner} spender=${approval.spender} amount=${approval.amount.toString()}`, raw };
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : "decode failed";
    return unknownStep(index, addressHex, topic0Hex, raw, message);
  }

  return unknownStep(index, addressHex, topic0Hex, raw, "unsupported topic0");
}

function decodeAaveFlashLoan(log: ReceiptView["logs"][number], topic0Hex: string): string {
  const decoded = decodeAaveFlashLoanFields(log, topic0Hex);
  return `flash borrow(${decoded.version}) pool=${toHexLower(log.address)} asset=${decoded.asset} amount=${decoded.amount.toString()} premium=${decoded.premium.toString()} initiator=${decoded.initiator}`;
}

function decodeAaveFlashLoanFields(
  log: ReceiptView["logs"][number],
  topic0Hex: string
): { version: "v2" | "v3" | "simple"; asset: string; amount: bigint; premium: bigint; initiator: string } {
  if (topic0Hex === TOPIC_AAVE_V2_FLASHLOAN) {
    return {
      version: "v2",
      asset: readAddressFromIndexed(log, 3),
      amount: readWord(log.data, 0),
      premium: readWord(log.data, 1),
      initiator: readAddressFromIndexed(log, 2),
    };
  }
  if (topic0Hex === TOPIC_AAVE_V3_FLASHLOAN) {
    return {
      version: "v3",
      asset: readAddressFromData(log.data, 0),
      amount: readWord(log.data, 1),
      premium: readWord(log.data, 3),
      initiator: readAddressFromIndexed(log, 2),
    };
  }
  if (topic0Hex === TOPIC_AAVE_V3_FLASHLOAN_SIMPLE) {
    return {
      version: "simple",
      asset: readAddressFromIndexed(log, 3),
      amount: readWord(log.data, 0),
      premium: readWord(log.data, 1),
      initiator: readAddressFromIndexed(log, 2),
    };
  }
  throw new Error("unsupported aave flashloan topic");
}

function decodeUniswapV2(log: ReceiptView["logs"][number], topic0Hex: string): string {
  if (topic0Hex === TOPIC_UNIV2_SWAP) {
    const sender = readAddressFromIndexed(log, 1);
    const to = readAddressFromIndexed(log, 2);
    const amount0In = readWord(log.data, 0);
    const amount1In = readWord(log.data, 1);
    const amount0Out = readWord(log.data, 2);
    const amount1Out = readWord(log.data, 3);
    return `swap(v2) pair=${toHexLower(log.address)} sender=${sender} to=${to} amount0In=${amount0In.toString()} amount1In=${amount1In.toString()} amount0Out=${amount0Out.toString()} amount1Out=${amount1Out.toString()}`;
  }
  if (topic0Hex === TOPIC_UNIV2_MINT) {
    const sender = readAddressFromIndexed(log, 1);
    const amount0 = readWord(log.data, 0);
    const amount1 = readWord(log.data, 1);
    return `mint(v2) pair=${toHexLower(log.address)} sender=${sender} amount0=${amount0.toString()} amount1=${amount1.toString()}`;
  }
  const sender = readAddressFromIndexed(log, 1);
  const to = readAddressFromIndexed(log, 2);
  const amount0 = readWord(log.data, 0);
  const amount1 = readWord(log.data, 1);
  return `burn(v2) pair=${toHexLower(log.address)} sender=${sender} to=${to} amount0=${amount0.toString()} amount1=${amount1.toString()}`;
}

function decodeUniswapV3Swap(log: ReceiptView["logs"][number]): string {
  const sender = readAddressFromIndexed(log, 1);
  const recipient = readAddressFromIndexed(log, 2);
  const amount0 = readSignedWord(log.data, 0);
  const amount1 = readSignedWord(log.data, 1);
  return `swap(v3) pool=${toHexLower(log.address)} sender=${sender} recipient=${recipient} amount0=${amount0.toString()} amount1=${amount1.toString()}`;
}

function decodeErc20Transfer(log: ReceiptView["logs"][number]): { from: string; to: string; amount: bigint } {
  return {
    from: readAddressFromIndexed(log, 1),
    to: readAddressFromIndexed(log, 2),
    amount: readWord(log.data, 0),
  };
}

function decodeErc20Approval(log: ReceiptView["logs"][number]): { owner: string; spender: string; amount: bigint } {
  return {
    owner: readAddressFromIndexed(log, 1),
    spender: readAddressFromIndexed(log, 2),
    amount: readWord(log.data, 0),
  };
}

function unknownStep(
  index: number,
  addressHex: string,
  topic0Hex: string | null,
  raw: TimelineStep["raw"],
  reason: string
): TimelineStep {
  return { index, type: "unknown", protocol: "unknown", addressHex, topic0Hex, summary: `unknown event (${reason})`, raw };
}

function readWord(data: Uint8Array, wordIndex: number): bigint {
  const start = wordIndex * 32;
  const end = start + 32;
  if (data.length < end) {
    throw new Error(`data word ${wordIndex} is missing`);
  }
  let out = 0n;
  for (let i = start; i < end; i += 1) {
    const byte = data[i];
    if (byte === undefined) {
      throw new Error(`data byte ${i} is missing`);
    }
    out = (out << 8n) + BigInt(byte);
  }
  return out;
}

function readSignedWord(data: Uint8Array, wordIndex: number): bigint {
  const raw = readWord(data, wordIndex);
  const signBit = 1n << 255n;
  if ((raw & signBit) === 0n) {
    return raw;
  }
  const mod = 1n << 256n;
  return raw - mod;
}

function readAddressFromIndexed(log: ReceiptView["logs"][number], topicIndex: number): string {
  const topic = log.topics[topicIndex];
  if (!topic || topic.length !== 32) {
    throw new Error(`indexed topic ${topicIndex} is missing or malformed`);
  }
  return toHexLower(topic.subarray(12));
}

function readAddressFromData(data: Uint8Array, wordIndex: number): string {
  const start = wordIndex * 32 + 12;
  const end = start + 20;
  if (data.length < end) {
    throw new Error(`address data word ${wordIndex} is missing`);
  }
  return toHexLower(data.subarray(start, end));
}
