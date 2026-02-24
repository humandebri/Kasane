// どこで: Gatewayテスト / 何を: hex規約とJSON-RPCバリデーションを検証 / なぜ: 互換フォーマットの退行を防ぐため

import assert from "node:assert/strict";
import { generateKeyPairSync } from "node:crypto";
import { bytesToQuantity, parseDataHex, parseQuantityHex, toDataHex, toQuantityHex } from "../src/hex";
import { handleRpc } from "../src/handlers";
import { computeDepth, validateRequest } from "../src/jsonrpc";
import {
  __test_classify_call_object_err_code,
  __test_map_receipt,
  __test_map_block,
  __test_normalize_storage_slot32,
  __test_parse_call_object,
  __test_revert_data_hex,
  __test_resolve_submitted_eth_hash_from_lookup,
  __test_as_call_params,
  __test_as_tx_count_params,
  __test_map_get_logs_error,
  __test_parse_logs_filter,
  __test_tx_hash_readiness_error,
  __test_to_candid_call_object,
  __test_map_tx,
} from "../src/handlers";
import { loadConfig } from "../src/config";
import { identityFromPem } from "../src/identity";
import { __test_resolve_cors_allow_origin } from "../src/server";

function testHex(): void {
  assert.equal(toDataHex(Uint8Array.from([0, 1, 255])), "0x0001ff");
  assert.equal(toQuantityHex(0n), "0x0");
  assert.equal(toQuantityHex(255n), "0xff");
  assert.equal(parseQuantityHex("0xff"), 255n);
  assert.equal(bytesToQuantity(Uint8Array.from([0, 0, 1])), 1n);
  assert.throws(() => parseDataHex("0xabc"));
  assert.throws(() => parseQuantityHex("0x00"));
}

function testJsonRpc(): void {
  const ok = validateRequest({ jsonrpc: "2.0", id: 1, method: "eth_chainId", params: [] });
  assert.ok(ok);
  const bad = validateRequest({ jsonrpc: "2.0", id: {}, method: "eth_chainId" });
  assert.equal(bad, null);
  const depth = computeDepth({ a: [{ b: [1] }] });
  assert.equal(depth, 5);
}

function testConfigIdentityPemPath(): void {
  const withPem = loadConfig({
    EVM_CANISTER_ID: "aaaaa-aa",
    RPC_GATEWAY_IDENTITY_PEM_PATH: " /tmp/rpc-gateway.pem ",
  });
  assert.equal(withPem.identityPemPath, "/tmp/rpc-gateway.pem");

  const withoutPem = loadConfig({
    EVM_CANISTER_ID: "aaaaa-aa",
    RPC_GATEWAY_IDENTITY_PEM_PATH: "   ",
  });
  assert.equal(withoutPem.identityPemPath, null);
}

function testConfigCorsOrigins(): void {
  const defaults = loadConfig({
    EVM_CANISTER_ID: "aaaaa-aa",
  });
  assert.deepEqual(defaults.corsOrigins, ["*"]);

  const single = loadConfig({
    EVM_CANISTER_ID: "aaaaa-aa",
    RPC_GATEWAY_CORS_ORIGIN: "https://kasane.network",
  });
  assert.deepEqual(single.corsOrigins, ["https://kasane.network"]);

  const multi = loadConfig({
    EVM_CANISTER_ID: "aaaaa-aa",
    RPC_GATEWAY_CORS_ORIGIN: "https://kasane.network, http://localhost:3000",
  });
  assert.deepEqual(multi.corsOrigins, ["https://kasane.network", "http://localhost:3000"]);
}

function testCorsAllowOriginResolution(): void {
  assert.equal(__test_resolve_cors_allow_origin("http://localhost:3000", ["*"]), "*");
  assert.equal(
    __test_resolve_cors_allow_origin("http://localhost:3000", [
      "https://kasane.network",
      "http://localhost:3000",
    ]),
    "http://localhost:3000"
  );
  assert.equal(
    __test_resolve_cors_allow_origin("http://localhost:3001", ["https://kasane.network", "http://localhost:3000"]),
    null
  );
  assert.equal(__test_resolve_cors_allow_origin(undefined, ["https://kasane.network"]), null);
}

function testIdentityFromEd25519Pem(): void {
  const pair = generateKeyPairSync("ed25519");
  const pem = pair.privateKey.export({ format: "pem", type: "pkcs8" }).toString();
  const identity = identityFromPem(pem);
  assert.notEqual(identity.getPrincipal().toText(), "2vxsx-fae");
}

function testCallParamsDefaultBlockTag(): void {
  const [callOnly, defaultTag] = __test_as_call_params([{ to: "0x0000000000000000000000000000000000000000" }]);
  assert.equal(typeof callOnly, "object");
  assert.equal(defaultTag, "latest");

  const [, explicitTag] = __test_as_call_params([{ to: "0x0000000000000000000000000000000000000000" }, "pending"]);
  assert.equal(explicitTag, "pending");

  assert.throws(() => __test_as_call_params([]));
}

function testTxCountParamsDefaultBlockTag(): void {
  const [addressOnly, defaultTag] = __test_as_tx_count_params([
    "0x0000000000000000000000000000000000000000",
  ]);
  assert.equal(addressOnly, "0x0000000000000000000000000000000000000000");
  assert.equal(defaultTag, "latest");

  const [, explicitTag] = __test_as_tx_count_params([
    "0x0000000000000000000000000000000000000000",
    "pending",
  ]);
  assert.equal(explicitTag, "pending");

  assert.throws(() => __test_as_tx_count_params([]));
}

function testCallObjectParsing(): void {
  const ok = __test_parse_call_object({
    to: "0x0000000000000000000000000000000000000000",
    gas: "0x5208",
    gasPrice: "0x1",
    value: "0x0",
    data: "0x",
    nonce: "0x2",
    chainId: "0x1",
    type: "0x0",
  });
  assert.ok(!("error" in ok));
  if ("error" in ok) {
    throw new Error("call object parse failed");
  }
  const out = __test_to_candid_call_object(ok);
  assert.equal(out.to.length, 1);
  assert.equal(out.gas.length, 1);
  assert.equal(out.gas_price.length, 1);
  assert.equal(out.value.length, 1);
  assert.equal(out.nonce.length, 1);
  assert.equal(out.chain_id.length, 1);
  assert.equal(out.tx_type.length, 1);
  const value0 = out.value[0];
  assert.ok(value0);
  assert.equal(value0.length, 32);

  const eip1559 = __test_parse_call_object({
    to: "0x0000000000000000000000000000000000000000",
    maxFeePerGas: "0x10",
    maxPriorityFeePerGas: "0x1",
    type: "0x2",
    accessList: [
      {
        address: "0x0000000000000000000000000000000000000001",
        storageKeys: ["0x0000000000000000000000000000000000000000000000000000000000000000"],
      },
    ],
  });
  assert.ok(!("error" in eip1559));
  if ("error" in eip1559) {
    throw new Error("eip1559 call object parse failed");
  }
  const eipOut = __test_to_candid_call_object(eip1559);
  assert.equal(eipOut.max_fee_per_gas.length, 1);
  assert.equal(eipOut.max_priority_fee_per_gas.length, 1);
  assert.equal(eipOut.tx_type.length, 1);
  assert.equal(eipOut.access_list.length, 1);

  const ng = __test_parse_call_object({ gasPrice: "0x1", maxFeePerGas: "0x2" });
  assert.ok("error" in ng);
  if ("error" in ng) {
    assert.equal(ng.error, "gasPrice and maxFeePerGas/maxPriorityFeePerGas cannot be used together");
  }

  const ng2 = __test_parse_call_object({ maxPriorityFeePerGas: "0x1" });
  assert.ok("error" in ng2);
  if ("error" in ng2) {
    assert.equal(ng2.error, "maxPriorityFeePerGas requires maxFeePerGas");
  }

  const ng3 = __test_parse_call_object({ type: "0x1" });
  assert.ok("error" in ng3);
  if ("error" in ng3) {
    assert.equal(ng3.error, "type must be 0x0 or 0x2");
  }

  const ng4 = __test_parse_call_object({
    accessList: [{ address: "0x00", storageKeys: [] }],
  });
  assert.ok(!("error" in ng4));
  if ("error" in ng4) {
    throw new Error("accessList parse failed");
  }
  assert.throws(() => __test_to_candid_call_object(ng4));
}

function testStorageSlotNormalization(): void {
  const slotFromQuantity = __test_normalize_storage_slot32("0x0");
  assert.equal(slotFromQuantity.length, 32);
  assert.equal(toDataHex(slotFromQuantity), "0x0000000000000000000000000000000000000000000000000000000000000000");

  const slotData32 = "0x1111111111111111111111111111111111111111111111111111111111111111";
  const slotFromData = __test_normalize_storage_slot32(slotData32);
  assert.equal(toDataHex(slotFromData), slotData32);

  assert.throws(() => __test_normalize_storage_slot32(`0x1${"0".repeat(64)}`));
}

function testRevertDataFormat(): void {
  assert.equal(__test_revert_data_hex([]), "0x");
  assert.equal(__test_revert_data_hex([parseDataHex("0x08c379a0")]), "0x08c379a0");
}

function testCanisterErrorClassification(): void {
  assert.equal(__test_classify_call_object_err_code(1001), -32602);
  assert.equal(__test_classify_call_object_err_code(1999), -32602);
  assert.equal(__test_classify_call_object_err_code(2001), -32000);
  assert.equal(__test_classify_call_object_err_code(9999), -32000);
}

function testReceiptLogMapping(): void {
  const mapped = __test_map_receipt(
    {
      effective_gas_price: 1n,
      status: 1,
      l1_data_fee: 0n,
      tx_index: 2,
      logs: [
        {
          log_index: 7,
          address: Uint8Array.from(Buffer.from("11".repeat(20), "hex")),
          topics: [Uint8Array.from(Buffer.from("22".repeat(32), "hex"))],
          data: Uint8Array.from([0xaa, 0xbb]),
        },
      ],
      total_fee: 0n,
      block_number: 5n,
      operator_fee: 0n,
      eth_tx_hash: [Uint8Array.from(Buffer.from("33".repeat(32), "hex"))],
      gas_used: 21_000n,
      contract_address: [],
      tx_hash: Uint8Array.from(Buffer.from("44".repeat(32), "hex")),
    },
    Uint8Array.from(Buffer.from("55".repeat(32), "hex"))
  );
  const logs = mapped.logs as Array<Record<string, unknown>>;
  assert.equal(logs.length, 1);
  const log0 = logs[0];
  assert.ok(log0);
  assert.equal(log0.address, `0x${"11".repeat(20)}`);
  assert.equal(log0.blockNumber, "0x5");
  assert.equal(log0.transactionIndex, "0x2");
  assert.equal(log0.logIndex, "0x7");
}

function testBlockMappingWithFeeMetadata(): void {
  const beneficiary = Uint8Array.from(Buffer.from("44".repeat(20), "hex"));
  const mapped = __test_map_block(
    {
      txs: { Hashes: [] },
      block_hash: Uint8Array.from(Buffer.from("11".repeat(32), "hex")),
      number: 7n,
      timestamp: 1_770_000_000n,
      beneficiary,
      state_root: Uint8Array.from(Buffer.from("22".repeat(32), "hex")),
      parent_hash: Uint8Array.from(Buffer.from("33".repeat(32), "hex")),
      base_fee_per_gas: [250_000_000_000n],
      gas_limit: [3_000_000n],
      gas_used: [24_000n],
    },
    false
  );
  assert.ok("value" in mapped);
  if (!("value" in mapped)) {
    throw new Error("block mapping should succeed");
  }
  assert.equal(mapped.value.baseFeePerGas, "0x3a35294400");
  assert.equal(mapped.value.gasLimit, "0x2dc6c0");
  assert.equal(mapped.value.gasUsed, "0x5dc0");
  assert.equal(mapped.value.miner, "0x" + "44".repeat(20));
}

function testBlockMappingRejectsLegacyMetadata(): void {
  const mapped = __test_map_block(
    {
      txs: { Hashes: [] },
      block_hash: Uint8Array.from(Buffer.from("11".repeat(32), "hex")),
      number: 7n,
      timestamp: 1_770_000_000n,
      beneficiary: Uint8Array.from(Buffer.from("44".repeat(20), "hex")),
      state_root: Uint8Array.from(Buffer.from("22".repeat(32), "hex")),
      parent_hash: Uint8Array.from(Buffer.from("33".repeat(32), "hex")),
      base_fee_per_gas: [],
      gas_limit: [3_000_000n],
      gas_used: [24_000n],
    },
    false
  );
  assert.ok("error" in mapped);
}

function testEip1559GasPriceFallback(): void {
  const mapped = __test_map_tx({
    raw: Uint8Array.from([]),
    tx_index: [0],
    decode_ok: true,
    hash: Uint8Array.from(Buffer.from("11".repeat(32), "hex")),
    kind: { EthSigned: null },
    block_number: [7n],
    eth_tx_hash: [Uint8Array.from(Buffer.from("22".repeat(32), "hex"))],
    decoded: [
      {
        from: Uint8Array.from(Buffer.from("33".repeat(20), "hex")),
        to: [Uint8Array.from(Buffer.from("44".repeat(20), "hex"))],
        nonce: 1n,
        value: Uint8Array.from(new Array(31).fill(0).concat([1])),
        input: Uint8Array.from([]),
        gas_limit: 21_000n,
        gas_price: [],
        max_fee_per_gas: [16n],
        max_priority_fee_per_gas: [1n],
        chain_id: [1n],
      },
    ],
  });
  assert.equal(mapped.gasPrice, "0x10");
  assert.equal(mapped.maxFeePerGas, "0x10");
  assert.equal(mapped.maxPriorityFeePerGas, "0x1");
}

function testSubmitEthHashResolutionPolicy(): void {
  const notFound = __test_resolve_submitted_eth_hash_from_lookup([]);
  assert.equal(notFound.ok, false);
  if (!notFound.ok) {
    assert.equal(notFound.reason, "tx_id_not_found");
  }

  const missingEthHash = __test_resolve_submitted_eth_hash_from_lookup([
    {
      raw: Uint8Array.from([]),
      tx_index: [],
      decode_ok: false,
      hash: Uint8Array.from(Buffer.from("44".repeat(32), "hex")),
      kind: { EthSigned: null },
      block_number: [],
      eth_tx_hash: [],
      decoded: [],
    },
  ]);
  assert.equal(missingEthHash.ok, false);
  if (!missingEthHash.ok) {
    assert.equal(missingEthHash.reason, "eth_signed_missing_eth_tx_hash");
  }

  const resolved = __test_resolve_submitted_eth_hash_from_lookup([
    {
      raw: Uint8Array.from([]),
      tx_index: [],
      decode_ok: false,
      hash: Uint8Array.from(Buffer.from("55".repeat(32), "hex")),
      kind: { EthSigned: null },
      block_number: [],
      eth_tx_hash: [Uint8Array.from(Buffer.from("66".repeat(32), "hex"))],
      decoded: [],
    },
  ]);
  assert.equal(resolved.ok, true);
  if (resolved.ok) {
    assert.equal(toDataHex(resolved.hash), `0x${"66".repeat(32)}`);
  }
}

function testTxHashReadinessPolicy(): void {
  const migrating = __test_tx_hash_readiness_error(null, {
    needs_migration: true,
    critical_corrupt: false,
    schema_version: 5,
  });
  assert.ok(migrating);
  if (!migrating || !("error" in migrating)) {
    throw new Error("migration status should produce json-rpc error");
  }
  assert.equal(migrating.error.code, -32000);
  assert.equal(migrating.error.message, "state unavailable");
  assert.equal(
    JSON.stringify(migrating.error.data),
    JSON.stringify({ reason: "ops.read.needs_migration", schema_version: 5 })
  );

  const corrupt = __test_tx_hash_readiness_error(1, {
    needs_migration: false,
    critical_corrupt: true,
    schema_version: 5,
  });
  assert.ok(corrupt);
  if (!corrupt || !("error" in corrupt)) {
    throw new Error("corrupt status should produce json-rpc error");
  }
  assert.equal(corrupt.error.code, -32000);
  assert.equal(
    JSON.stringify(corrupt.error.data),
    JSON.stringify({ reason: "ops.read.critical_corrupt", schema_version: 5 })
  );

  const ready = __test_tx_hash_readiness_error(1, {
    needs_migration: false,
    critical_corrupt: false,
    schema_version: 5,
  });
  assert.equal(ready, null);
}

async function testGetLogsFilterParsing(): Promise<void> {
  const parsed = await __test_parse_logs_filter(
    {
      fromBlock: "earliest",
      toBlock: "latest",
      address: "0x0000000000000000000000000000000000000001",
      topics: [
        "0x1111111111111111111111111111111111111111111111111111111111111111",
        null,
      ],
    },
    99n
  );
  assert.ok(!("error" in parsed));
  if ("error" in parsed) {
    throw new Error(String(parsed.error));
  }
  assert.equal(parsed.value.candid.from_block.length, 1);
  assert.equal(parsed.value.candid.to_block.length, 1);
  assert.equal(parsed.value.candid.address.length, 1);
  assert.equal(parsed.value.candid.topic0.length, 1);
  assert.equal(parsed.value.candid.topic1.length, 0);

  const ng = await __test_parse_logs_filter({ topics: ["0x11"] }, 1n);
  assert.ok("error" in ng);
  if ("error" in ng) {
    assert.ok(ng.error.includes("topics[0]"));
  }

  const ng2 = await __test_parse_logs_filter({ blockHash: `0x${"00".repeat(32)}` }, 1n);
  assert.ok("error" in ng2);
  if ("error" in ng2) {
    assert.equal(ng2.error, "blockHash filter is not supported");
  }

  const ng3 = await __test_parse_logs_filter(
    {
      topics: [
        "0x1111111111111111111111111111111111111111111111111111111111111111",
        "0x2222222222222222222222222222222222222222222222222222222222222222",
      ],
    },
    1n
  );
  assert.ok("error" in ng3);
  if ("error" in ng3) {
    assert.equal(ng3.error, "only topics[0] is supported");
  }
}

function testGetLogsErrorMapping(): void {
  const invalid = __test_map_get_logs_error({ InvalidArgument: "bad filter" });
  assert.equal(invalid.code, -32602);
  assert.equal(invalid.message, "invalid params");
  const range = __test_map_get_logs_error({ RangeTooLarge: null });
  assert.equal(range.code, -32005);
  assert.equal(range.message, "limit exceeded");
}

async function testInvalidTxHashReturnsInvalidParams(): Promise<void> {
  const txByHashRes = await handleRpc({
    jsonrpc: "2.0",
    id: 1,
    method: "eth_getTransactionByHash",
    params: ["0x1234"],
  });
  assert.ok(txByHashRes);
  if (!txByHashRes || !("error" in txByHashRes)) {
    throw new Error("eth_getTransactionByHash invalid hash should return error");
  }
  assert.equal(txByHashRes.error.code, -32602);

  const receiptRes = await handleRpc({
    jsonrpc: "2.0",
    id: 2,
    method: "eth_getTransactionReceipt",
    params: ["0x1234"],
  });
  assert.ok(receiptRes);
  if (!receiptRes || !("error" in receiptRes)) {
    throw new Error("eth_getTransactionReceipt invalid hash should return error");
  }
  assert.equal(receiptRes.error.code, -32602);
}

testHex();
testJsonRpc();
testConfigIdentityPemPath();
testConfigCorsOrigins();
testCorsAllowOriginResolution();
testIdentityFromEd25519Pem();
testCallParamsDefaultBlockTag();
testTxCountParamsDefaultBlockTag();
testCallObjectParsing();
testStorageSlotNormalization();
testRevertDataFormat();
testCanisterErrorClassification();
testReceiptLogMapping();
testBlockMappingWithFeeMetadata();
testBlockMappingRejectsLegacyMetadata();
testEip1559GasPriceFallback();
testSubmitEthHashResolutionPolicy();
testTxHashReadinessPolicy();
testGetLogsErrorMapping();

async function main(): Promise<void> {
  await testInvalidTxHashReturnsInvalidParams();
  await testGetLogsFilterParsing();
  console.log("ok");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
