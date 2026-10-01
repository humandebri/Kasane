import { Pool } from "pg";
export async function seedFixtureDatabase(databaseUrl: string) {
  const pool = new Pool({ connectionString: databaseUrl });
  await pool.query(`
    CREATE TABLE blocks(number bigint primary key, hash bytea, timestamp bigint not null, tx_count integer not null, gas_used bigint);
    CREATE TABLE txs(tx_hash bytea primary key, eth_tx_hash bytea, block_number bigint not null, tx_index integer not null, caller_principal bytea, from_address bytea not null, to_address bytea, tx_selector bytea, receipt_status smallint, internal_trace_failed boolean not null default false, internal_trace_truncated boolean not null default false, internal_trace_captured_count integer, internal_trace_total_count integer);
    CREATE TABLE tx_receipts_index(tx_hash bytea primary key, contract_address bytea, status smallint not null, block_number bigint not null, tx_index integer not null);
    CREATE TABLE internal_transactions(tx_hash bytea not null, block_number bigint not null, tx_index integer not null, trace_id text not null, trace_sort_key text not null, depth integer not null, action_type text not null, from_address bytea not null, to_address bytea, created_contract_address bytea, value_numeric numeric(78,0) not null, success boolean not null, error_code text, primary key(tx_hash, trace_id));
    CREATE TABLE token_transfers(tx_hash bytea not null, block_number bigint not null, tx_index integer not null, log_index integer not null, token_address bytea not null, from_address bytea not null, to_address bytea not null, amount_numeric numeric(78,0) not null, primary key(tx_hash, log_index));
    CREATE TABLE verified_contracts(id text primary key, contract_address text not null, chain_id integer not null, contract_name text not null, compiler_version text not null, optimizer_enabled boolean not null, optimizer_runs integer not null, evm_version text, creation_match boolean not null, runtime_match boolean not null, abi_json text not null, source_blob_id text not null, metadata_blob_id text not null, published_at bigint not null);
    CREATE TABLE verify_blobs(id text primary key, encoding text not null, raw_size integer not null, blob bytea not null);
    CREATE TABLE metrics_daily(day integer primary key, raw_bytes bigint not null default 0, compressed_bytes bigint not null default 0, archive_bytes bigint, blocks_ingested bigint not null default 0, errors bigint not null default 0);
    CREATE TABLE ops_metrics_samples(sampled_at_ms bigint primary key, queue_len bigint not null, cycles bigint not null default 0, pruned_before_block bigint, estimated_kept_bytes bigint, low_water_bytes bigint, high_water_bytes bigint, hard_emergency_bytes bigint, total_submitted bigint not null, total_included bigint not null, total_dropped bigint not null, drop_counts_json text not null);
    CREATE TABLE meta(key text primary key, value text);

CREATE TABLE verify_metrics_samples(sampled_at_ms bigint primary key, queue_depth bigint not null, success_count bigint not null, failed_count bigint not null, avg_duration_ms bigint not null, p50_duration_ms bigint not null, p95_duration_ms bigint not null, fail_by_code_json text not null);
`);
  const from = Buffer.from("11".repeat(20), "hex");
  const to = Buffer.from("22".repeat(20), "hex");
  const hash = Buffer.from("aa".repeat(32), "hex");
  await pool.query("INSERT INTO blocks VALUES($1,$2,$3,$4,$5)", [12, Buffer.from("bb".repeat(32), "hex"), 1000, 1, 21000]);
  await pool.query("INSERT INTO txs(tx_hash,block_number,tx_index,from_address,to_address,tx_selector,receipt_status) VALUES($1,$2,$3,$4,$5,$6,$7)", [hash,12,0,from,to,Buffer.from("a9059cbb","hex"),1]);
  await pool.query("INSERT INTO metrics_daily(day,raw_bytes,compressed_bytes,blocks_ingested) VALUES($1,$2,$3,$4)", [20260101,1000,500,1]);
  await pool.end();

}

if (process.argv[1]?.endsWith("fixture-db.ts")) {
  const databaseUrl = process.env.EXPLORER_TEST_DATABASE_URL;
  if (!databaseUrl) throw new Error("EXPLORER_TEST_DATABASE_URL is required (an empty disposable database)");
  await seedFixtureDatabase(databaseUrl);
  console.log("fixture database seeded");
}
