// どこで: Explorerテスト / 何を: hex変換とDBクエリを検証 / なぜ: 最小実装の退行を防ぐため

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import Database from "better-sqlite3";
import { ExplorerDb } from "../lib/db";
import { parseHex, toHexLower } from "../lib/hex";

function runHexTests(): void {
  const bytes = parseHex("0x00aabb");
  assert.equal(bytes.length, 3);
  assert.equal(toHexLower(bytes), "0x00aabb");
  assert.throws(() => parseHex("0xabc"));
  assert.throws(() => parseHex("0xzz"));
}

function runDbTests(): void {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "ic-op-explorer-"));
  const dbFile = path.join(dir, "test.sqlite");

  const seed = new Database(dbFile);
  seed.exec(
    "CREATE TABLE blocks(number integer primary key, hash blob, timestamp integer not null, tx_count integer not null);" +
      "CREATE TABLE txs(tx_hash blob primary key, block_number integer not null, tx_index integer not null);"
  );
  seed.prepare("INSERT INTO blocks(number, hash, timestamp, tx_count) VALUES(?, ?, ?, ?)").run(12, Buffer.from("aa", "hex"), 1000, 1);
  seed.prepare("INSERT INTO blocks(number, hash, timestamp, tx_count) VALUES(?, ?, ?, ?)").run(11, Buffer.from("bb", "hex"), 900, 1);
  seed.prepare("INSERT INTO txs(tx_hash, block_number, tx_index) VALUES(?, ?, ?)").run(
    Buffer.from("1122", "hex"),
    12,
    0
  );
  seed.close();

  const db = new ExplorerDb(dbFile);
  assert.equal(db.getMaxBlockNumber(), 12n);
  const latestBlock = db.getLatestBlocks(1)[0];
  const latestTx = db.getLatestTxs(1)[0];
  assert.ok(latestBlock);
  assert.ok(latestTx);
  assert.equal(latestBlock.number, 12n);
  assert.equal(latestTx.txHashHex, "0x1122");
  assert.equal(db.getBlockDetails(12n)?.txs.length, 1);
  assert.equal(db.getTx(Uint8Array.from(Buffer.from("1122", "hex")))?.blockNumber, 12n);
  db.close();

  fs.rmSync(dir, { recursive: true, force: true });
}

runHexTests();
runDbTests();
console.log("ok");
