// どこで: Tx監視ビュー / 何を: send受理とreceipt実行結果を分離して状態判定 / なぜ: 監視運用で誤判定を避けるため

import { isTxHashHex, normalizeHex, parseHex, toHexLower } from "./hex";
import {
  getRpcPending,
  getRpcReceiptWithStatus,
  getRpcTxByEthHash,
  getRpcTxByTxId,
  type PendingStatusView,
  type RpcReceiptLookupView,
  type RpcTxView,
} from "./rpc";

export type TxMonitorState =
  | "accepted_not_included"
  | "included_success"
  | "included_failed"
  | "dropped"
  | "unknown_or_pruned";

export type TxMonitorView = {
  inputHashHex: string;
  resolvedEthTxHashHex: string | null;
  txIdHex: string | null;
  state: TxMonitorState;
  summary: string;
  tx: RpcTxView | null;
  receipt: RpcReceiptLookupView;
  pending: PendingStatusView | null;
};

export async function getTxMonitorView(inputHashHex: string): Promise<TxMonitorView> {
  if (!isTxHashHex(inputHashHex)) {
    throw new Error("tx hash must be 32-byte hex");
  }
  const normalized = normalizeHex(inputHashHex);
  const inputHash = parseHex(normalized);
  const txByEthHash = await getRpcTxByEthHash(inputHash);
  const resolved = await resolveHashInputs(inputHash, txByEthHash);
  const [tx, receipt] = await Promise.all([
    resolved.tx,
    getRpcReceiptWithStatus(resolved.receiptLookupHash),
  ]);

  if ("Found" in receipt) {
    const status = receipt.Found.status;
    return {
      inputHashHex: normalized,
      resolvedEthTxHashHex: toHexLower(resolved.receiptLookupHash),
      txIdHex: tx ? toHexLower(tx.hash) : toHexLower(receipt.Found.tx_hash),
      state: status === 1 ? "included_success" : "included_failed",
      summary: status === 1 ? "receipt.status=1" : "receipt.status=0",
      tx,
      receipt,
      pending: null,
    };
  }

  const txId = tx ? tx.hash : null;
  const pending = txId ? await getRpcPending(txId) : null;
  const pendingResolved = resolvePendingState(pending);

  if (pendingResolved.state === "accepted_not_included") {
    return {
      inputHashHex: normalized,
      resolvedEthTxHashHex: firstEthHashHex(tx),
      txIdHex: txId ? toHexLower(txId) : null,
      state: pendingResolved.state,
      summary: pendingResolved.summary,
      tx,
      receipt,
      pending,
    };
  }
  if (pendingResolved.state === "dropped") {
    return {
      inputHashHex: normalized,
      resolvedEthTxHashHex: firstEthHashHex(tx),
      txIdHex: txId ? toHexLower(txId) : null,
      state: "dropped",
      summary: pendingResolved.summary,
      tx,
      receipt,
      pending,
    };
  }

  if ("Pruned" in receipt) {
    return {
      inputHashHex: normalized,
      resolvedEthTxHashHex: firstEthHashHex(tx),
      txIdHex: txId ? toHexLower(txId) : null,
      state: "unknown_or_pruned",
      summary: `receipt pruned before block ${receipt.Pruned.pruned_before_block.toString()}`,
      tx,
      receipt,
      pending,
    };
  }
  if ("PossiblyPruned" in receipt) {
    return {
      inputHashHex: normalized,
      resolvedEthTxHashHex: firstEthHashHex(tx),
      txIdHex: txId ? toHexLower(txId) : null,
      state: "unknown_or_pruned",
      summary: `receipt possibly pruned before block ${receipt.PossiblyPruned.pruned_before_block.toString()}`,
      tx,
      receipt,
      pending,
    };
  }

  return {
    inputHashHex: normalized,
    resolvedEthTxHashHex: firstEthHashHex(tx),
    txIdHex: txId ? toHexLower(txId) : null,
    state: "unknown_or_pruned",
    summary: tx ? "tx metadata exists but receipt/pending are unresolved" : "tx not found",
    tx,
    receipt,
    pending,
  };
}

function firstEthHashHex(tx: RpcTxView | null): string | null {
  if (!tx || tx.eth_tx_hash.length === 0) {
    return null;
  }
  const first = tx.eth_tx_hash[0];
  return first ? toHexLower(first) : null;
}

async function resolveHashInputs(
  inputHash: Uint8Array,
  txByEthHash: RpcTxView | null
): Promise<{ tx: RpcTxView | null; receiptLookupHash: Uint8Array }> {
  if (txByEthHash) {
    return { tx: txByEthHash, receiptLookupHash: inputHash };
  }
  const txByTxId = await getRpcTxByTxId(inputHash);
  if (!txByTxId) {
    return { tx: null, receiptLookupHash: inputHash };
  }
  if (txByTxId.eth_tx_hash.length > 0) {
    const ethHash = txByTxId.eth_tx_hash[0];
    if (ethHash) {
      return { tx: txByTxId, receiptLookupHash: ethHash };
    }
  }
  return { tx: txByTxId, receiptLookupHash: inputHash };
}

function resolvePendingState(
  pending: PendingStatusView | null
): { state: "accepted_not_included" | "dropped" | "unknown"; summary: string } {
  if (!pending || "Unknown" in pending || "Included" in pending) {
    return { state: "unknown", summary: "pending status is unknown" };
  }
  if ("Queued" in pending) {
    return {
      state: "accepted_not_included",
      summary: `queued (seq=${pending.Queued.seq.toString()})`,
    };
  }
  return {
    state: "dropped",
    summary: `dropped (code=${pending.Dropped.code})`,
  };
}
