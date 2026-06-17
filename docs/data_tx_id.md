# `data_tx_id`

This document records the current `tx_id`, hash, and storage semantics. It describes the implementation as it exists, not a new design.

## Current `tx_id` Definition

All routes use the common internal `stored_tx_id`.

```text
tx_id = keccak256(
  "ic-evm:storedtx:v2" ||
  kind_u8 ||
  raw_tx_bytes ||
  optional(caller_evm[20]) ||
  optional(u16be(canister_id_len) || canister_id_bytes) ||
  optional(u16be(caller_principal_len) || caller_principal_bytes)
)
```

- `kind_u8 = 0x01`: `EthSigned`
- `kind_u8 = 0x02`: `IcSynthetic`
- Implementation: `crates/evm-core/src/hash.rs` (`stored_tx_id`)

## Route A: `EthSigned`

`submit_tx` uses `stored_tx_id(kind=EthSigned, raw, None, None, None)`. The internal `tx_id` is not the Ethereum transaction hash.

`eth_tx_hash = keccak256(raw_tx_bytes)` is stored separately in `eth_tx_hash_index` as `eth_tx_hash -> tx_id`. Hash-based Ethereum RPC methods resolve through that index.

## Route B: `IcSynthetic`

`submit_ic_tx(record)` uses `stored_tx_id(kind=IcSynthetic, raw, caller_evm, canister_id, caller_principal)`.

- `caller_evm` is derived from `caller_principal`.
- Nonce is not auto-assigned by the canister; the submitted `nonce` field is used as-is.

## Hash Rules

```text
tx_list_hash = keccak256(0x00 || tx_id_0 || tx_id_1 || ...)
```

```text
block_hash = keccak256(
  0x01 ||
  parent_hash(32) ||
  number(u64 be) ||
  timestamp(u64 be) ||
  tx_list_hash(32) ||
  state_root(32)
)
```

The first byte is the domain-separation prefix.

## Stable Schema Summary

- Root: `StableState` (`StableBTreeMap` groups and `StableCell` groups).
- Main maps: `queue`, `tx_store`, `tx_locs`, `tx_locs_v3`, `blocks`, `receipts`, `eth_tx_hash_index`.
- Main cells: `chain_state`, `head`, `queue_meta`.

## API Effects

- `submit_raw_tx` and `submit_tx` return internal `tx_id`.
- `rpc_eth_get_transaction_by_hash` resolves through `eth_tx_hash_index`.
- `get_pending` and `get_receipt` use internal `tx_id`.

## References

- `crates/evm-core/src/hash.rs`
- `crates/evm-core/src/chain.rs`
- `crates/ic-evm-rpc/src/lib.rs`
- `crates/evm-db/src/stable_state.rs`
- `crates/evm-db/src/chain_data/tx.rs`
