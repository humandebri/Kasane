// where: mainnet method test helper
// what: execute query methods through @dfinity/agent and emit machine-readable results
// why: avoid icp canister call for query paths and keep repeatable evidence for reports

import { Actor, HttpAgent } from "@dfinity/agent";
import { pathToFileURL } from "node:url";
import fs from "node:fs";

const canisterId = process.env.EVM_CANISTER_ID;
if (!canisterId) {
  throw new Error("missing env: EVM_CANISTER_ID");
}
const host = process.env.INDEXER_IC_HOST || "https://icp-api.io";
const didJs = process.env.EVM_DID_JS;
if (!didJs) {
  throw new Error("missing env: EVM_DID_JS");
}
const queryOut = process.env.QUERY_OUT || "";
const summaryOut = process.env.QUERY_SUMMARY_OUT || "";
const fetchRootKey = process.env.INDEXER_FETCH_ROOT_KEY === "true";

function toHex(bytes) {
  return Buffer.from(bytes).toString("hex");
}

function replacer(_key, value) {
  if (typeof value === "bigint") {
    return value.toString();
  }
  if (value instanceof Uint8Array) {
    return { __hex: toHex(value) };
  }
  return value;
}

function shortValue(value) {
  const text = JSON.stringify(value, replacer);
  if (!text) {
    return "null";
  }
  if (text.length <= 220) {
    return text;
  }
  return `${text.slice(0, 220)}...`;
}

function lineWrite(text) {
  console.log(text);
  if (queryOut) {
    fs.appendFileSync(queryOut, `${text}\n`);
  }
}

async function callQuery(actor, name, args, meta = {}) {
  const startedAt = new Date().toISOString();
  try {
    const fn = actor[name];
    if (typeof fn !== "function") {
      throw new Error(`method not found on actor: ${name}`);
    }
    const value = await fn(...args);
    const row = {
      method: name,
      ok: true,
      started_at: startedAt,
      summary: shortValue(value),
      value,
      ...meta,
    };
    lineWrite(JSON.stringify(row, replacer));
    return row;
  } catch (error) {
    const row = {
      method: name,
      ok: false,
      started_at: startedAt,
      error: error instanceof Error ? error.message : String(error),
      ...meta,
    };
    lineWrite(JSON.stringify(row));
    return row;
  }
}

async function main() {
  const didModule = await import(pathToFileURL(didJs).href);
  const idlFactory = didModule.idlFactory;
  if (typeof idlFactory !== "function") {
    throw new Error("idlFactory not found in EVM_DID_JS");
  }

  const agent = new HttpAgent({ host, fetch: globalThis.fetch });
  if (fetchRootKey) {
    await agent.fetchRootKey();
  }
  const actor = Actor.createActor(idlFactory, { agent, canisterId });

  const zero20 = new Uint8Array(20);
  const zero32 = new Uint8Array(32);
  const emptyCall = {
    to: [],
    gas: [],
    value: [],
    max_priority_fee_per_gas: [],
    data: [],
    from: [],
    max_fee_per_gas: [],
    chain_id: [],
    nonce: [],
    tx_type: [],
    access_list: [],
    gas_price: [],
  };
  const defaultLogFilter = {
    from_block: [],
    to_block: [],
    address: [],
    topic0: [],
    topic1: [],
    limit: [10],
  };

  const baselineCalls = [
    ["expected_nonce_by_address", [zero20]],
    ["export_blocks", [[], 65536]],
    ["get_block", [0n]],
    ["get_block", [1n]],
    ["get_cycle_balance", []],
    ["get_miner_allowlist", []],
    ["get_ops_status", []],
    ["get_pending", [zero32]],
    ["get_prune_status", []],
    ["get_queue_snapshot", [10, []]],
    ["get_receipt", [zero32]],
    ["health", []],
    ["metrics", [60n]],
    ["metrics_prometheus", []],
    ["rpc_eth_block_number", []],
    ["rpc_eth_call_object", [emptyCall]],
    ["rpc_eth_call_rawtx", [new Uint8Array(0)]],
    ["rpc_eth_chain_id", []],
    ["rpc_eth_estimate_gas_object", [emptyCall]],
    ["rpc_eth_get_balance", [zero20]],
    ["rpc_eth_get_block_by_number", [0n, false]],
    ["rpc_eth_get_block_by_number_with_status", [0n, false]],
    ["rpc_eth_get_code", [zero20]],
    ["rpc_eth_get_logs_paged", [defaultLogFilter, [], 10]],
    ["rpc_eth_get_storage_at", [zero20, zero32]],
    ["rpc_eth_get_transaction_by_eth_hash", [zero32]],
    ["rpc_eth_get_transaction_by_tx_id", [zero32]],
    ["rpc_eth_get_transaction_receipt_by_eth_hash", [zero32]],
    ["rpc_eth_get_transaction_receipt_with_status", [zero32]],
  ];

  const rows = [];
  for (const [name, args] of baselineCalls) {
    // eslint-disable-next-line no-await-in-loop
    rows.push(await callQuery(actor, name, args));
  }

  const extraTxIdHex = process.env.EXTRA_TX_ID_HEX || "";
  if (extraTxIdHex) {
    const txId = Uint8Array.from(Buffer.from(extraTxIdHex, "hex"));
    rows.push(await callQuery(actor, "get_pending", [txId]));
    rows.push(await callQuery(actor, "get_receipt", [txId]));
    rows.push(await callQuery(actor, "rpc_eth_get_transaction_by_tx_id", [txId]));
  }

  const extraEthHashHex = process.env.EXTRA_ETH_HASH_HEX || "";
  if (extraEthHashHex) {
    const ethHash = Uint8Array.from(Buffer.from(extraEthHashHex, "hex"));
    rows.push(await callQuery(actor, "rpc_eth_get_transaction_by_eth_hash", [ethHash]));
    rows.push(await callQuery(actor, "rpc_eth_get_transaction_receipt_by_eth_hash", [ethHash]));
    rows.push(await callQuery(actor, "rpc_eth_get_transaction_receipt_with_status", [ethHash]));
  }

  const extraAddressHex = process.env.EXTRA_ADDRESS_HEX || "";
  if (extraAddressHex) {
    const address = Uint8Array.from(Buffer.from(extraAddressHex, "hex"));
    rows.push(await callQuery(actor, "expected_nonce_by_address", [address], { address_hex: extraAddressHex.toLowerCase() }));
  }

  if (summaryOut) {
    const summary = {
      get_ops_status: null,
      get_miner_allowlist: null,
      rpc_eth_chain_id: null,
    };
    for (const row of rows) {
      if (!row.ok) {
        continue;
      }
      if (row.method === "get_ops_status") {
        summary.get_ops_status = row.value;
      }
      if (row.method === "get_miner_allowlist") {
        summary.get_miner_allowlist = row.value;
      }
      if (row.method === "rpc_eth_chain_id") {
        summary.rpc_eth_chain_id = row.value;
      }
    }
    fs.writeFileSync(summaryOut, JSON.stringify(summary, replacer, 2));
  }
}

main().catch((err) => {
  console.error(`[query-matrix] failed: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
});
