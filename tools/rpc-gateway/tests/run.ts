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
  __test_receipt_hash_matches,
  __test_normalize_storage_slot32,
  __test_parse_call_object,
  __test_revert_data_hex,
  __test_resolve_submitted_eth_hash_from_lookup,
  __test_as_call_params,
  __test_as_tx_count_params,
  __test_compute_effective_priority_fee,
  __test_compute_next_base_fee,
  __test_compute_weighted_percentile,
  __test_is_latest_tag,
  __test_map_get_logs_error,
  __test_parse_logs_filter,
  __test_parse_reward_percentiles,
  __test_parse_execution_block_tag,
  __test_parse_fee_history_params,
  __test_map_rpc_error,
  __test_sort_log_items,
  __test_tx_hash_readiness_error,
  __test_to_candid_call_object,
  __test_map_tx,
} from "../src/handlers";
import { loadConfig } from "../src/config";
import { __test_assert_canister_compatibility, __test_create_retryable_promise_cache } from "../src/client";
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

function testConfigLogsBlockhashScanLimit(): void {
  const defaults = loadConfig({
    EVM_CANISTER_ID: "aaaaa-aa",
  });
  assert.equal(defaults.logsBlockhashScanLimit, 2000);

  const custom = loadConfig({
    EVM_CANISTER_ID: "aaaaa-aa",
    RPC_GATEWAY_LOGS_BLOCKHASH_SCAN_LIMIT: "1500",
  });
  assert.equal(custom.logsBlockhashScanLimit, 1500);

  const invalidLow = loadConfig({
    EVM_CANISTER_ID: "aaaaa-aa",
    RPC_GATEWAY_LOGS_BLOCKHASH_SCAN_LIMIT: "50",
  });
  assert.equal(invalidLow.logsBlockhashScanLimit, 2000);
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

function testLatestTagNormalization(): void {
  assert.equal(__test_is_latest_tag("latest"), true);
  assert.equal(__test_is_latest_tag(" Latest "), true);
  assert.equal(__test_is_latest_tag(new String("pending")), true);
  assert.equal(__test_is_latest_tag({ blockNumber: "safe" }), true);
  assert.equal(__test_is_latest_tag("0x1"), false);
  assert.equal(__test_is_latest_tag({ blockHash: `0x${"11".repeat(32)}` }), false);
}

function testExecutionTagNormalization(): void {
  assert.deepEqual(__test_parse_execution_block_tag("latest"), { Latest: null });
  assert.deepEqual(__test_parse_execution_block_tag("pending"), { Pending: null });
  assert.deepEqual(__test_parse_execution_block_tag("safe"), { Safe: null });
  assert.deepEqual(__test_parse_execution_block_tag("finalized"), { Finalized: null });
  assert.deepEqual(__test_parse_execution_block_tag("earliest"), { Earliest: null });
  assert.deepEqual(__test_parse_execution_block_tag("0x10"), { Number: 16n });
  assert.throws(() => __test_parse_execution_block_tag("final"));
  assert.throws(() => __test_parse_execution_block_tag(1));
}

function testPriorityFeeComputation(): void {
  const eip1559Tip = __test_compute_effective_priority_fee(
    {
      from: Uint8Array.from(Buffer.from("11".repeat(20), "hex")),
      to: [],
      value: Uint8Array.from(new Array(32).fill(0)),
      chain_id: [],
      nonce: 1n,
      gas_limit: 21_000n,
      input: Uint8Array.from([]),
      gas_price: [],
      max_fee_per_gas: [100n],
      max_priority_fee_per_gas: [5n],
    },
    97n
  );
  assert.equal(eip1559Tip, 3n);

  const legacyTip = __test_compute_effective_priority_fee(
    {
      from: Uint8Array.from(Buffer.from("11".repeat(20), "hex")),
      to: [],
      value: Uint8Array.from(new Array(32).fill(0)),
      chain_id: [],
      nonce: 1n,
      gas_limit: 21_000n,
      input: Uint8Array.from([]),
      gas_price: [120n],
      max_fee_per_gas: [],
      max_priority_fee_per_gas: [],
    },
    100n
  );
  assert.equal(legacyTip, 20n);
}

function testWeightedPercentileAndNextBaseFee(): void {
  const p50 = __test_compute_weighted_percentile(
    [
      { tip: 1n, gasLimit: 1n },
      { tip: 5n, gasLimit: 10n },
      { tip: 9n, gasLimit: 1n },
    ],
    50
  );
  assert.equal(p50, 5n);

  const next = __test_compute_next_base_fee(100n, 12_000n, 20_000n);
  assert.equal(next, 102n);
}

function testRewardPercentilesValidation(): void {
  assert.deepEqual(__test_parse_reward_percentiles(undefined), null);
  assert.deepEqual(__test_parse_reward_percentiles([10, 50, 90]), [10, 50, 90]);
  assert.throws(() => __test_parse_reward_percentiles([50, 10]));
  assert.throws(() => __test_parse_reward_percentiles([101]));
}

function testFeeHistoryBlockCountCompatibility(): void {
  const fromNumber = __test_parse_fee_history_params([5, "latest"]);
  assert.equal(fromNumber.blockCount, 5n);
  assert.deepEqual(fromNumber.newestTag, { Latest: null });

  const fromHex = __test_parse_fee_history_params(["0x5", "latest"]);
  assert.equal(fromHex.blockCount, 5n);

  const fromDecimal = __test_parse_fee_history_params(["5", "latest"]);
  assert.equal(fromDecimal.blockCount, 5n);

  assert.throws(() => __test_parse_fee_history_params([0, "latest"]));
  assert.throws(() => __test_parse_fee_history_params([-1, "latest"]));
  assert.throws(() => __test_parse_fee_history_params([1.5, "latest"]));
  assert.throws(() => __test_parse_fee_history_params([Number.NaN, "latest"]));
  assert.throws(() => __test_parse_fee_history_params([Number.POSITIVE_INFINITY, "latest"]));
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
    gasLimit: "21000",
    maxFeePerGas: "0x10",
    maxPriorityFeePerGas: "1",
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
  assert.equal(eipOut.gas.length, 1);

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
      to: [Uint8Array.from(Buffer.from("77".repeat(20), "hex"))],
      effective_gas_price: 1n,
      status: 1,
      l1_data_fee: 0n,
      tx_index: 2,
      block_hash: [Uint8Array.from(Buffer.from("88".repeat(32), "hex"))],
      from: [Uint8Array.from(Buffer.from("66".repeat(20), "hex"))],
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
  assert.equal(mapped.blockHash, `0x${"88".repeat(32)}`);
  assert.equal(mapped.from, `0x${"66".repeat(20)}`);
  assert.equal(mapped.to, `0x${"77".repeat(20)}`);
  const logs = mapped.logs as Array<Record<string, unknown>>;
  assert.equal(logs.length, 1);
  const log0 = logs[0];
  assert.ok(log0);
  assert.equal(log0.address, `0x${"11".repeat(20)}`);
  assert.equal(log0.blockNumber, "0x5");
  assert.equal(log0.blockHash, `0x${"88".repeat(32)}`);
  assert.equal(log0.transactionIndex, "0x2");
  assert.equal(log0.logIndex, "0x7");
}

function testReceiptHashStrictMatch(): void {
  const requested = Uint8Array.from(Buffer.from("aa".repeat(32), "hex"));
  const mappedMatch = __test_map_receipt(
    {
      to: [Uint8Array.from(Buffer.from("77".repeat(20), "hex"))],
      effective_gas_price: 1n,
      status: 1,
      l1_data_fee: 0n,
      tx_index: 2,
      block_hash: [Uint8Array.from(Buffer.from("88".repeat(32), "hex"))],
      from: [Uint8Array.from(Buffer.from("66".repeat(20), "hex"))],
      logs: [],
      total_fee: 0n,
      block_number: 5n,
      operator_fee: 0n,
      eth_tx_hash: [requested],
      gas_used: 21_000n,
      contract_address: [],
      tx_hash: Uint8Array.from(Buffer.from("44".repeat(32), "hex")),
    },
    Uint8Array.from(Buffer.from("55".repeat(32), "hex"))
  );
  assert.equal(__test_receipt_hash_matches(mappedMatch, requested), true);

  const mappedMismatch = __test_map_receipt(
    {
      to: [Uint8Array.from(Buffer.from("77".repeat(20), "hex"))],
      effective_gas_price: 1n,
      status: 1,
      l1_data_fee: 0n,
      tx_index: 2,
      block_hash: [Uint8Array.from(Buffer.from("88".repeat(32), "hex"))],
      from: [Uint8Array.from(Buffer.from("66".repeat(20), "hex"))],
      logs: [],
      total_fee: 0n,
      block_number: 5n,
      operator_fee: 0n,
      eth_tx_hash: [Uint8Array.from(Buffer.from("bb".repeat(32), "hex"))],
      gas_used: 21_000n,
      contract_address: [],
      tx_hash: Uint8Array.from(Buffer.from("44".repeat(32), "hex")),
    },
    Uint8Array.from(Buffer.from("55".repeat(32), "hex"))
  );
  assert.equal(__test_receipt_hash_matches(mappedMismatch, requested), false);
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
    block_hash: [],
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
      block_hash: [],
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
      block_hash: [],
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
  assert.equal(parsed.value.blockHash.length, 0);
  assert.equal(parsed.value.candidFilters.length, 1);
  assert.equal(parsed.value.candidFilters[0]?.from_block.length, 1);
  assert.equal(parsed.value.candidFilters[0]?.to_block.length, 1);
  assert.equal(parsed.value.candidFilters[0]?.address.length, 1);
  assert.equal(parsed.value.candidFilters[0]?.topic0.length, 1);
  assert.equal(parsed.value.candidFilters[0]?.topic1.length, 0);

  const ng = await __test_parse_logs_filter({ topics: ["0x11"] }, 1n);
  assert.ok("error" in ng);
  if ("error" in ng) {
    assert.ok(ng.error.includes("topics[0]"));
  }

  const withBlockHash = await __test_parse_logs_filter({ blockHash: `0x${"00".repeat(32)}` }, 1n);
  assert.ok(!("error" in withBlockHash));
  if (!("error" in withBlockHash)) {
    assert.equal(withBlockHash.value.blockHash.length, 1);
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

  const ng4 = await __test_parse_logs_filter(
    {
      blockHash: `0x${"00".repeat(32)}`,
      fromBlock: "0x1",
    },
    1n
  );
  assert.ok("error" in ng4);
  if ("error" in ng4) {
    assert.equal(ng4.error, "blockHash cannot be combined with fromBlock/toBlock");
  }

  const withBlockHashAndNullRange = await __test_parse_logs_filter(
    {
      blockHash: `0x${"00".repeat(32)}`,
      fromBlock: null,
      toBlock: null,
    },
    1n
  );
  assert.ok(!("error" in withBlockHashAndNullRange));

  const withOrTopic = await __test_parse_logs_filter(
    {
      topics: [
        [
          `0x${"11".repeat(32)}`,
          `0x${"22".repeat(32)}`,
        ],
      ],
    },
    1n
  );
  assert.ok(!("error" in withOrTopic));
  if (!("error" in withOrTopic)) {
    assert.equal(withOrTopic.value.candidFilters.length, 2);
    assert.equal(withOrTopic.value.candidFilters[0]?.topic0.length, 1);
    assert.equal(withOrTopic.value.candidFilters[1]?.topic0.length, 1);
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

function testLogSortOrder(): void {
  const sorted = __test_sort_log_items([
    {
      tx_index: 2,
      log_index: 1,
      data: Uint8Array.from([0x02]),
      block_number: 10n,
      topics: [],
      address: Uint8Array.from(new Array(20).fill(0x11)),
      eth_tx_hash: [],
      tx_hash: Uint8Array.from(new Array(32).fill(0xaa)),
    },
    {
      tx_index: 0,
      log_index: 0,
      data: Uint8Array.from([0x00]),
      block_number: 9n,
      topics: [],
      address: Uint8Array.from(new Array(20).fill(0x11)),
      eth_tx_hash: [],
      tx_hash: Uint8Array.from(new Array(32).fill(0xbb)),
    },
    {
      tx_index: 2,
      log_index: 0,
      data: Uint8Array.from([0x01]),
      block_number: 10n,
      topics: [],
      address: Uint8Array.from(new Array(20).fill(0x11)),
      eth_tx_hash: [],
      tx_hash: Uint8Array.from(new Array(32).fill(0xcc)),
    },
  ]);
  assert.equal(sorted[0]?.block_number, 9n);
  assert.equal(sorted[1]?.log_index, 0);
  assert.equal(sorted[2]?.log_index, 1);
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

async function testCanisterCompatibilityProbe(): Promise<void> {
  await __test_assert_canister_compatibility({
    rpc_eth_history_window: async () => ({ oldest_available: 0n, latest: 0n }),
    rpc_eth_gas_price: async () => ({ Ok: 1n }),
  });

  await assert.rejects(
    __test_assert_canister_compatibility({}),
    /incompatible\.canister\.api/
  );

  await assert.rejects(
    __test_assert_canister_compatibility({
      rpc_eth_history_window: async () => {
        throw new Error("Method does not exist");
      },
      rpc_eth_gas_price: async () => ({ Ok: 1n }),
    }),
    /incompatible\.canister\.api/
  );

  await assert.rejects(
    __test_assert_canister_compatibility({
      rpc_eth_history_window: async () => ({ oldest_available: 0n, latest: 0n }),
      rpc_eth_gas_price: async () => {
        throw new Error("Method does not exist");
      },
    }),
    /incompatible\.canister\.api/
  );
}

async function testRetryablePromiseCache(): Promise<void> {
  let attempts = 0;
  const cached = __test_create_retryable_promise_cache(async () => {
    attempts += 1;
    if (attempts === 1) {
      throw new Error("first-failure");
    }
    return "ok";
  });
  await assert.rejects(cached(), /first-failure/);
  const second = await cached();
  assert.equal(second, "ok");
  assert.equal(attempts, 2);
}

function testRpcErrorPrefixPassthrough(): void {
  const mapped = __test_map_rpc_error(
    1,
    {
      code: 2001,
      message: "exec.state.unavailable historical nonce is unavailable",
      error_prefix: ["exec.state.unavailable"],
    },
    "state unavailable"
  );
  assert.ok("error" in mapped);
  if (!("error" in mapped)) {
    throw new Error("rpc error mapping should return error response");
  }
  assert.equal(mapped.error.code, -32000);
  assert.equal(mapped.error.message, "state unavailable");
  assert.deepEqual(mapped.error.data, {
    detail: "exec.state.unavailable historical nonce is unavailable",
    rpc_code: 2001,
    error_prefix: "exec.state.unavailable",
  });
}

testHex();
testJsonRpc();
testConfigIdentityPemPath();
testConfigCorsOrigins();
testConfigLogsBlockhashScanLimit();
testCorsAllowOriginResolution();
testIdentityFromEd25519Pem();
testCallParamsDefaultBlockTag();
testTxCountParamsDefaultBlockTag();
testLatestTagNormalization();
testExecutionTagNormalization();
testCallObjectParsing();
testStorageSlotNormalization();
testRevertDataFormat();
testCanisterErrorClassification();
testPriorityFeeComputation();
testWeightedPercentileAndNextBaseFee();
testReceiptHashStrictMatch();
testRewardPercentilesValidation();
testFeeHistoryBlockCountCompatibility();
testRpcErrorPrefixPassthrough();
testReceiptLogMapping();
testBlockMappingWithFeeMetadata();
testBlockMappingRejectsLegacyMetadata();
testEip1559GasPriceFallback();
testSubmitEthHashResolutionPolicy();
testTxHashReadinessPolicy();
testGetLogsErrorMapping();
testLogSortOrder();

async function main(): Promise<void> {
  await testInvalidTxHashReturnsInvalidParams();
  await testGetLogsFilterParsing();
  await testCanisterCompatibilityProbe();
  await testRetryablePromiseCache();
  console.log("ok");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
