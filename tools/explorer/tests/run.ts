// どこで: Explorerテスト / 何を: hex変換とPostgresクエリを検証 / なぜ: Postgres移行後の退行を防ぐため

import assert from "node:assert/strict";
import { newDb } from "pg-mem";
import { parseHex, toHexLower } from "../lib/hex";
import {
  closeExplorerPool,
  getBlockDetails,
  getLatestBlocks,
  getLatestTxs,
  getMaxBlockNumber,
  getTx,
  setExplorerPool,
} from "../lib/db";

async function runHexTests(): Promise<void> {
  const bytes = parseHex("0x00aabb");
  assert.equal(bytes.length, 3);
  assert.equal(toHexLower(bytes), "0x00aabb");
  assert.throws(() => parseHex("0xabc"));
  assert.throws(() => parseHex("0xzz"));
}

async function runDbTests(): Promise<void> {
  const mem = newDb({ noAstCoverageCheck: true });
  mem.public.none(`
    CREATE TABLE blocks(number bigint primary key, hash bytea, timestamp bigint not null, tx_count integer not null);
    CREATE TABLE txs(tx_hash bytea primary key, block_number bigint not null, tx_index integer not null);
  `);

  const adapter = mem.adapters.createPg();
  const pool = new adapter.Pool();
  setExplorerPool(pool);
  await pool.query("INSERT INTO blocks(number, hash, timestamp, tx_count) VALUES($1, $2, $3, $4)", [12, Buffer.from("aa", "hex"), 1000, 1]);
  await pool.query("INSERT INTO blocks(number, hash, timestamp, tx_count) VALUES($1, $2, $3, $4)", [11, Buffer.from("bb", "hex"), 900, 1]);
  await pool.query("INSERT INTO txs(tx_hash, block_number, tx_index) VALUES($1, $2, $3)", [Buffer.from("1122", "hex"), 12, 0]);

  const head = await getMaxBlockNumber();
  assert.equal(head, 12n);
  const latestBlock = (await getLatestBlocks(1))[0];
  const latestTx = (await getLatestTxs(1))[0];
  assert.ok(latestBlock);
  assert.ok(latestTx);
  assert.equal(latestBlock?.number, 12n);
  assert.equal(latestTx?.txHashHex, "0x1122");
  assert.equal((await getBlockDetails(12n))?.txs.length, 1);
  assert.equal((await getTx(Uint8Array.from(Buffer.from("1122", "hex"))))?.blockNumber, 12n);

  await closeExplorerPool();
}

runHexTests()
  .then(runDbTests)
  .then(() => {
    console.log("ok");
  })
  .catch((err) => {
    console.error(err);
    process.exit(1);
  });
