// どこで: Explorerテスト / 何を: hex変換とPostgresクエリを検証 / なぜ: Postgres移行後の退行を防ぐため

import assert from "node:assert/strict";
import { promises as fs } from "node:fs";
import { newDb } from "pg-mem";
import {
  isAddressHex,
  isTxHashHex,
  normalizeHex,
  parseAddressHex,
  parseHex,
  toHexLower,
} from "../lib/hex";
import {
  closeExplorerPool,
  getBlockDetails,
  getLatestBlocks,
  getLatestTxs,
  getMaxBlockNumber,
  getMetaSnapshot,
  getRecentOpsMetricsSamples,
  getOverviewStats,
  getTx,
  getTxsByAddress,
  getTxsByCallerPrincipal,
  setExplorerPool,
} from "../lib/db";
import { getPrincipalView } from "../lib/data";
import { parseStoredPruneStatusForTest } from "../lib/data_ops";
import { deriveEvmAddressFromPrincipal } from "../lib/principal";
import { logsTestHooks } from "../lib/logs";
import { resolveSearchRoute } from "../lib/search";

async function runHexTests(): Promise<void> {
  const bytes = parseHex("0x00aabb");
  assert.equal(bytes.length, 3);
  assert.equal(toHexLower(bytes), "0x00aabb");
  assert.throws(() => parseHex("0xabc"));
  assert.throws(() => parseHex("0xzz"));
  assert.equal(normalizeHex("AABB"), "0xaabb");
  assert.equal(isAddressHex("0x0000000000000000000000000000000000000000"), true);
  assert.equal(isAddressHex("0x00"), false);
  assert.equal(isTxHashHex("0x" + "11".repeat(32)), true);
  assert.equal(isTxHashHex("0x" + "11".repeat(20)), false);
  assert.equal(parseAddressHex("0x" + "22".repeat(20)).length, 20);
  assert.throws(() => parseAddressHex("0x" + "22".repeat(19)));
}

async function runSearchTests(): Promise<void> {
  assert.equal(resolveSearchRoute("12"), "/blocks/12");
  assert.equal(resolveSearchRoute("0x" + "11".repeat(32)), "/tx/0x" + "11".repeat(32));
  assert.equal(resolveSearchRoute("0x" + "22".repeat(20)), "/address/0x" + "22".repeat(20));
  assert.equal(
    resolveSearchRoute("2vxsx-fae"),
    "/principal/2vxsx-fae"
  );
  assert.equal(resolveSearchRoute("invalid"), "/");
}

async function runPrincipalDeriveTests(): Promise<void> {
  const address = deriveEvmAddressFromPrincipal("nggqm-p5ozz-i5hfv-bejmq-2gtow-4dtqw-vjatn-4b4yw-s5mzs-i46su-6ae");
  assert.equal(address, "0xf53e047376e37eac56d48245b725c47410cf6f1e");
}

async function runDependencyPinTests(): Promise<void> {
  const packageJsonRaw = await fs.readFile(new URL("../package.json", import.meta.url), "utf8");
  const packageJson = JSON.parse(packageJsonRaw) as {
    dependencies?: Record<string, string>;
  };
  const pinned = packageJson.dependencies?.["@dfinity/ic-pub-key"];
  assert.equal(pinned, "1.0.1");

  const lockRaw = await fs.readFile(new URL("../package-lock.json", import.meta.url), "utf8");
  const lockJson = JSON.parse(lockRaw) as {
    packages?: Record<string, { version?: string }>;
  };
  const lockVersion = lockJson.packages?.["node_modules/@dfinity/ic-pub-key"]?.version;
  assert.equal(lockVersion, "1.0.1");
}

async function runLogsTests(): Promise<void> {
  const topic1Unsupported = logsTestHooks.parseFilter({
    fromBlock: "",
    toBlock: "",
    address: "",
    topic0: "",
    topic1: "0x" + "11".repeat(32),
    blockHash: "",
    limit: "200",
  });
  assert.equal(topic1Unsupported.ok, false);
  if (topic1Unsupported.ok) {
    throw new Error("topic1 unsupported test expected error");
  }
  assert.equal(topic1Unsupported.error, "topic1 は未対応です（topic0 のみ指定してください）");

  const blockHashUnsupported = logsTestHooks.parseFilter({
    fromBlock: "",
    toBlock: "",
    address: "",
    topic0: "",
    topic1: "",
    blockHash: "0x" + "22".repeat(32),
    limit: "200",
  });
  assert.equal(blockHashUnsupported.ok, false);
  if (blockHashUnsupported.ok) {
    throw new Error("blockHash unsupported test expected error");
  }
  assert.equal(blockHashUnsupported.error, "blockHash フィルタは未対応です（from/to を使用してください）");
}

async function runDbTests(): Promise<void> {
  const mem = newDb({ noAstCoverageCheck: true });
  mem.public.none(`
    CREATE TABLE blocks(number bigint primary key, hash bytea, timestamp bigint not null, tx_count integer not null);
    CREATE TABLE txs(tx_hash bytea primary key, block_number bigint not null, tx_index integer not null, caller_principal bytea, from_address bytea not null, to_address bytea, receipt_status smallint);
    CREATE TABLE metrics_daily(day integer primary key, raw_bytes bigint not null default 0, compressed_bytes bigint not null default 0, archive_bytes bigint, blocks_ingested bigint not null default 0, errors bigint not null default 0);
    CREATE TABLE ops_metrics_samples(sampled_at_ms bigint primary key, queue_len bigint not null, total_submitted bigint not null, total_included bigint not null, total_dropped bigint not null, drop_counts_json text not null);
    CREATE TABLE meta(key text primary key, value text);
  `);

  const adapter = mem.adapters.createPg();
  const pool = new adapter.Pool();
  setExplorerPool(pool);
  await pool.query("INSERT INTO blocks(number, hash, timestamp, tx_count) VALUES($1, $2, $3, $4)", [12, Buffer.from("aa", "hex"), 1000, 1]);
  await pool.query("INSERT INTO blocks(number, hash, timestamp, tx_count) VALUES($1, $2, $3, $4)", [11, Buffer.from("bb", "hex"), 900, 1]);
  await pool.query("INSERT INTO txs(tx_hash, block_number, tx_index, caller_principal, from_address, to_address, receipt_status) VALUES($1, $2, $3, $4, $5, $6, $7)", [
    Buffer.from("1122", "hex"),
    12,
    0,
    null,
    Buffer.from("11".repeat(20), "hex"),
    Buffer.from("22".repeat(20), "hex"),
    1,
  ]);
  await pool.query("INSERT INTO txs(tx_hash, block_number, tx_index, caller_principal, from_address, to_address, receipt_status) VALUES($1, $2, $3, $4, $5, $6, $7)", [
    Buffer.from("3344", "hex"),
    11,
    0,
    Buffer.from([4]),
    Buffer.from("22".repeat(20), "hex"),
    Buffer.from("11".repeat(20), "hex"),
    0,
  ]);
  await pool.query("INSERT INTO txs(tx_hash, block_number, tx_index, caller_principal, from_address, to_address, receipt_status) VALUES($1, $2, $3, $4, $5, $6, $7)", [
    Buffer.from("5566", "hex"),
    10,
    1,
    null,
    Buffer.from("11".repeat(20), "hex"),
    Buffer.from("11".repeat(20), "hex"),
    null,
  ]);
  await pool.query(
    "INSERT INTO ops_metrics_samples(sampled_at_ms, queue_len, total_submitted, total_included, total_dropped, drop_counts_json) VALUES($1, $2, $3, $4, $5, $6), ($7, $8, $9, $10, $11, $12)",
    [1000, 1, 10, 5, 2, "[]", 2000, 2, 12, 5, 3, '[{\"code\":1,\"count\":3}]']
  );
  await pool.query(
    "INSERT INTO metrics_daily(day, raw_bytes, compressed_bytes, archive_bytes, blocks_ingested, errors) VALUES($1, $2, $3, $4, $5, $6)",
    [20260215, 99, 55, 55, 2, 0]
  );
  await pool.query(
    "INSERT INTO meta(key, value) VALUES($1, $2), ($3, $4), ($5, $6), ($7, $8)",
    [
      "need_prune",
      "1",
      "last_head",
      "12",
      "last_ingest_at",
      "1700000000000",
      "prune_status",
      JSON.stringify({ v: 1, fetched_at_ms: "1700000000000", status: { need_prune: true } }),
    ]
  );

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
  assert.equal((await getTx(Uint8Array.from(Buffer.from("1122", "hex"))))?.receiptStatus, 1);
  const byPrincipal = await getTxsByCallerPrincipal(Uint8Array.from([4]), 10);
  assert.equal(byPrincipal.length, 1);
  assert.equal(byPrincipal[0]?.txHashHex, "0x3344");
  const prevDatabaseUrl = process.env.EXPLORER_DATABASE_URL;
  const prevPrincipalTxs = process.env.EXPLORER_PRINCIPAL_TXS;
  try {
    process.env.EXPLORER_DATABASE_URL = "postgres://test";
    process.env.EXPLORER_PRINCIPAL_TXS = "1";
    const principal = await getPrincipalView("2vxsx-fae");
    assert.equal(principal.principalText, "2vxsx-fae");
    assert.equal(principal.txs.length, 1);
  } finally {
    process.env.EXPLORER_DATABASE_URL = prevDatabaseUrl;
    process.env.EXPLORER_PRINCIPAL_TXS = prevPrincipalTxs;
  }
  assert.equal((await getOverviewStats()).latestDay, 20260215);
  const meta = await getMetaSnapshot();
  assert.equal(meta.needPrune, true);
  assert.equal(meta.lastHead, 12n);
  assert.equal(meta.lastIngestAtMs, 1700000000000n);
  assert.ok(meta.pruneStatusRaw);
  const address = Uint8Array.from(Buffer.from("11".repeat(20), "hex"));
  const txsByAddress = await getTxsByAddress(address, 2, null);
  assert.equal(txsByAddress.length, 3);
  assert.equal(txsByAddress[0]?.txHashHex, "0x1122");
  assert.equal(txsByAddress[1]?.txHashHex, "0x3344");
  const next = txsByAddress[1];
  assert.ok(next);
  const page2 = await getTxsByAddress(address, 2, {
    blockNumber: next!.blockNumber,
    txIndex: next!.txIndex,
    txHash: Uint8Array.from(Buffer.from(next!.txHashHex.slice(2), "hex")),
  });
  assert.equal(page2.length, 1);
  assert.equal(page2[0]?.txHashHex, "0x5566");
  const opsSamples = await getRecentOpsMetricsSamples(10);
  assert.equal(opsSamples.length, 2);
  assert.equal(opsSamples[0]?.sampledAtMs, 2000n);

  await closeExplorerPool();
}

async function runDataTests(): Promise<void> {
  const parsed = parseStoredPruneStatusForTest(
    JSON.stringify({
      fetched_at_ms: "1700000000000",
      status: {
        pruning_enabled: true,
        prune_running: false,
        need_prune: true,
        pruned_before_block: "100",
        oldest_kept_block: "101",
        oldest_kept_timestamp: "1700000000001",
        estimated_kept_bytes: "200",
        high_water_bytes: "300",
        low_water_bytes: "250",
        hard_emergency_bytes: "400",
        last_prune_at: "1700000000002",
      },
    })
  );
  assert.ok(parsed?.status);
  assert.equal(parsed?.status?.pruningEnabled, true);
  assert.equal(parsed?.status?.highWaterBytes, 300n);

  const invalid = parseStoredPruneStatusForTest(
    JSON.stringify({
      fetched_at_ms: "1700000000000",
      status: {
        pruning_enabled: "invalid",
        prune_running: false,
        need_prune: true,
        pruned_before_block: "100",
        oldest_kept_block: "101",
        oldest_kept_timestamp: "1700000000001",
        estimated_kept_bytes: "200",
        high_water_bytes: "300",
        low_water_bytes: "250",
        hard_emergency_bytes: "400",
        last_prune_at: "1700000000002",
      },
    })
  );
  assert.equal(invalid?.status, null);
  assert.equal(parseStoredPruneStatusForTest("{"), null);
}

runHexTests()
  .then(runSearchTests)
  .then(runPrincipalDeriveTests)
  .then(runDependencyPinTests)
  .then(runLogsTests)
  .then(runDbTests)
  .then(runDataTests)
  .then(() => {
    console.log("ok");
  })
  .catch((err) => {
    console.error(err);
    process.exit(1);
  });
