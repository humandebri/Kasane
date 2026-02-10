// どこで: Gatewayテスト / 何を: hex規約とJSON-RPCバリデーションを検証 / なぜ: 互換フォーマットの退行を防ぐため

import assert from "node:assert/strict";
import { bytesToQuantity, parseDataHex, parseQuantityHex, toDataHex, toQuantityHex } from "../src/hex";
import { computeDepth, validateRequest } from "../src/jsonrpc";
import {
  __test_classify_call_object_err_code,
  __test_normalize_storage_slot32,
  __test_parse_call_object,
  __test_revert_data_hex,
  __test_to_candid_call_object,
} from "../src/handlers";

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

testHex();
testJsonRpc();
testCallObjectParsing();
testStorageSlotNormalization();
testRevertDataFormat();
testCanisterErrorClassification();
console.log("ok");
