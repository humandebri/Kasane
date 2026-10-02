import assert from "node:assert/strict";
import { newDb } from "pg-mem";
import { setExplorerPool, closeExplorerPool } from "../lib/db";
import { POST as submit } from "../lib/http/verify-submit";
import { GET as status } from "../lib/http/verify-status";
import { GET as verified } from "../lib/http/verified-contract";
import { buildVerifyAuthToken } from "../lib/verify/token";
import { parseUrlSearch, stringifyUrlSearch } from "../lib/url-search";
import { validatePageInput } from "../lib/page-input";

assert.deepEqual(parseUrlSearch("?block=00012&limit=2&limit=3"), { block: "00012", limit: ["2", "3"] });
assert.equal(stringifyUrlSearch({ block: "00012", limit: ["2", "3"], absent: undefined }), "?block=00012&limit=2&limit=3");
assert.equal(Object.getPrototypeOf(parseUrlSearch("?__proto__=x")), Object.prototype);
assert.deepEqual(parseUrlSearch("?__proto__=x"), JSON.parse('{"__proto__":"x"}'));
assert.throws(() => validatePageInput({ number: 12 }, ["number"]), /invalid number/);
assert.throws(() => validatePageInput({ page: [12] }, [], ["page"]), /invalid page/);
assert.deepEqual(validatePageInput({ page: ["1", "2"] }, [], ["page"]), { page: ["1", "2"] });
const mem = newDb({ noAstCoverageCheck: true });
mem.public.none(`
    CREATE TABLE verify_auth_replay(jti text primary key, sub text not null, scope text not null, exp bigint not null, consumed_at bigint not null);
    CREATE TABLE verify_requests(
      id text primary key,
      contract_address text not null,
      chain_id integer not null,
      submitted_by text not null,
      status text not null,
      input_hash text not null,
      payload_compressed bytea not null,
      error_code text,
      error_message text,
      started_at bigint,
      finished_at bigint,
      attempts integer not null default 0,
      verified_contract_id text,
      created_at bigint not null,
      updated_at bigint not null
    );
    CREATE UNIQUE INDEX uq_verify_requests_submitted_input_hash ON verify_requests(submitted_by, input_hash);
    CREATE TABLE verify_metrics_samples(
      sampled_at_ms bigint primary key,
      queue_depth bigint not null,
      success_count bigint not null,
      failed_count bigint not null,
      avg_duration_ms bigint,
      p50_duration_ms bigint,
      p95_duration_ms bigint,
      fail_by_code_json text not null
    );
  `);
const { Pool } = mem.adapters.createPg();
const pool = new Pool();
setExplorerPool(pool);
process.env.EXPLORER_DATABASE_URL = "postgresql://localhost/test";
process.env.EXPLORER_VERIFY_AUTH_HMAC_KEYS = "test-key:test-secret";
process.env.EXPLORER_VERIFY_ENABLED = "0";
const request = (path: string, token?: string, body?: string) => new Request(`http://localhost${path}`, {
  method: body === undefined ? "GET" : "POST",
  headers: token ? { authorization: `Bearer ${token}` } : {},
  body,
});
const token = (sub: string, jti: string) => buildVerifyAuthToken({ kid: "test-key", secret: "test-secret", sub, jti, scope: "verify.submit", expSec: Math.floor(Date.now() / 1000) + 60 });
try {
  assert.equal((await submit(request("/api/verify/submit", undefined, "{}"))).status, 503);
  assert.equal((await status(request("/api/verify/status?id=job"))).status, 503);
  process.env.EXPLORER_VERIFY_ENABLED = "1";
  assert.equal((await submit(request("/api/verify/submit", undefined, "{}"))).status, 401);
  assert.equal((await status(request("/api/verify/status?id=job"))).status, 401);
  const invalidInputToken = token("owner", "invalid-input");
  assert.equal((await submit(request("/api/verify/submit", invalidInputToken, "{"))).status, 400);
  assert.equal((await submit(request("/api/verify/submit", invalidInputToken, "{"))).status, 401);
  const previousLimit = process.env.EXPLORER_VERIFY_MAX_PAYLOAD_BYTES;
  process.env.EXPLORER_VERIFY_MAX_PAYLOAD_BYTES = "1024";
  let bodyReads = 0;
  let bodyCancelled = false;
  const oversizedBody = new ReadableStream<Uint8Array>({
    pull(controller) { bodyReads++; controller.enqueue(new Uint8Array(1024)); },
    cancel() { bodyCancelled = true; },
  }, { highWaterMark: 0 });
  try {
    const oversized = new Request("http://localhost/api/verify/submit", {
      method: "POST",
      headers: { authorization: `Bearer ${token("owner", "oversized-input")}` },
      body: oversizedBody,
      duplex: "half",
    } as RequestInit);
    assert.equal((await submit(oversized)).status, 413);
    assert.equal(bodyReads, 2);
    assert.equal(bodyCancelled, true);
  } finally {
    if (previousLimit === undefined) delete process.env.EXPLORER_VERIFY_MAX_PAYLOAD_BYTES;
    else process.env.EXPLORER_VERIFY_MAX_PAYLOAD_BYTES = previousLimit;
  }
  await pool.query("INSERT INTO verify_requests(id,contract_address,chain_id,submitted_by,status,input_hash,payload_compressed,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)", ["job", "0x" + "11".repeat(20), 0, "owner", "queued", "hash", Buffer.from("payload"), 1, 1]);
  const ownerToken = token("owner", "status-repeatable");
  const response = await status(request("/api/verify/status?id=job", ownerToken));
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type") ?? "", /application\/json/);
  assert.equal((await response.json()).requestId, "job");
  assert.equal((await status(request("/api/verify/status?id=job", ownerToken))).status, 200);
  assert.equal((await status(request("/api/verify/status?id=job", token("other", "other")))).status, 403);
  assert.equal((await status(request("/api/verify/status?id=missing", ownerToken))).status, 404);
  assert.equal((await status(request("/api/verify/status", ownerToken))).status, 400);
  const invalidAddress = await verified(request("/api/contracts/invalid/verified"), { params: Promise.resolve({ address: "invalid" }) });
  assert.equal(invalidAddress.status, 400);
  assert.deepEqual(await invalidAddress.json(), { error: "invalid address" });
  const invalidChain = await verified(request("/api/contracts/a/verified?chainId=2147483648"), { params: Promise.resolve({ address: "0x" + "11".repeat(20) }) });
  assert.equal(invalidChain.status, 400);
  console.log("HTTP and URL regression tests passed");
} finally {
  await closeExplorerPool();
}
