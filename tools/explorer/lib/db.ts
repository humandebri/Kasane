// どこで: Explorer DB層 / 何を: SQLite読み取りクエリを集中管理 / なぜ: UI層と永続化層を分離して保守しやすくするため

import path from "node:path";
import Database from "better-sqlite3";

export type BlockSummary = {
  number: bigint;
  hashHex: string | null;
  timestamp: bigint;
  txCount: number;
};

export type TxSummary = {
  txHashHex: string;
  blockNumber: bigint;
  txIndex: number;
};

export type BlockDetails = {
  block: BlockSummary;
  txs: TxSummary[];
};

export class ExplorerDb {
  private readonly db: Database.Database;

  constructor(dbPath: string) {
    const resolved = path.resolve(dbPath);
    this.db = new Database(resolved, { readonly: true });
    this.db.pragma("query_only = ON");
  }

  close(): void {
    this.db.close();
  }

  getMaxBlockNumber(): bigint | null {
    const row = this.db.prepare("SELECT MAX(number) as number FROM blocks").get();
    const value = toBigInt((row as { number: number | bigint | null }).number);
    return value;
  }

  getLatestBlocks(limit: number): BlockSummary[] {
    const rows = this.db
      .prepare("SELECT number, hash, timestamp, tx_count FROM blocks ORDER BY number DESC LIMIT ?")
      .all(limit) as Array<{ number: number | bigint; hash: Buffer | null; timestamp: number | bigint; tx_count: number }>;

    return rows.map((row) => ({
      number: toBigIntRequired(row.number),
      hashHex: row.hash ? `0x${row.hash.toString("hex")}` : null,
      timestamp: toBigIntRequired(row.timestamp),
      txCount: row.tx_count,
    }));
  }

  getLatestTxs(limit: number): TxSummary[] {
    const rows = this.db
      .prepare("SELECT tx_hash, block_number, tx_index FROM txs ORDER BY block_number DESC, tx_index DESC LIMIT ?")
      .all(limit) as Array<{ tx_hash: Buffer; block_number: number | bigint; tx_index: number }>;

    return rows.map((row) => ({
      txHashHex: `0x${row.tx_hash.toString("hex")}`,
      blockNumber: toBigIntRequired(row.block_number),
      txIndex: row.tx_index,
    }));
  }

  getBlockDetails(blockNumber: bigint): BlockDetails | null {
    const blockRow = this.db
      .prepare("SELECT number, hash, timestamp, tx_count FROM blocks WHERE number = ?")
      .get(blockNumber) as { number: number | bigint; hash: Buffer | null; timestamp: number | bigint; tx_count: number } | undefined;

    if (!blockRow) {
      return null;
    }

    const txRows = this.db
      .prepare("SELECT tx_hash, block_number, tx_index FROM txs WHERE block_number = ? ORDER BY tx_index ASC")
      .all(blockNumber) as Array<{ tx_hash: Buffer; block_number: number | bigint; tx_index: number }>;

    return {
      block: {
        number: toBigIntRequired(blockRow.number),
        hashHex: blockRow.hash ? `0x${blockRow.hash.toString("hex")}` : null,
        timestamp: toBigIntRequired(blockRow.timestamp),
        txCount: blockRow.tx_count,
      },
      txs: txRows.map((row) => ({
        txHashHex: `0x${row.tx_hash.toString("hex")}`,
        blockNumber: toBigIntRequired(row.block_number),
        txIndex: row.tx_index,
      })),
    };
  }

  getTx(txHash: Uint8Array): TxSummary | null {
    const row = this.db
      .prepare("SELECT tx_hash, block_number, tx_index FROM txs WHERE tx_hash = ?")
      .get(Buffer.from(txHash)) as { tx_hash: Buffer; block_number: number | bigint; tx_index: number } | undefined;

    if (!row) {
      return null;
    }

    return {
      txHashHex: `0x${row.tx_hash.toString("hex")}`,
      blockNumber: toBigIntRequired(row.block_number),
      txIndex: row.tx_index,
    };
  }
}

function toBigInt(value: number | bigint | null): bigint | null {
  if (value === null) {
    return null;
  }
  return typeof value === "bigint" ? value : BigInt(value);
}

function toBigIntRequired(value: number | bigint): bigint {
  return typeof value === "bigint" ? value : BigInt(value);
}
