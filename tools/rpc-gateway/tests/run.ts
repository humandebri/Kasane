// どこで: Gatewayテスト / 何を: hex規約とJSON-RPCバリデーションを検証 / なぜ: 互換フォーマットの退行を防ぐため

import assert from "node:assert/strict";
import { bytesToQuantity, parseDataHex, parseQuantityHex, toDataHex, toQuantityHex } from "../src/hex";
import { computeDepth, validateRequest } from "../src/jsonrpc";

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

testHex();
testJsonRpc();
console.log("ok");
