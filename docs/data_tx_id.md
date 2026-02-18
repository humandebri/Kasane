# data_tx_id（現行実装準拠）

このドキュメントは、`tx_id` と関連ハッシュ/保存構造の「現在の実装」を簡潔に固定するためのものです。
設計案ではなく、実装追従の運用ドキュメントとして扱います。

## 1. tx_id の現行定義

`tx_id` は Route 別に別定義ではなく、内部的には共通で `stored_tx_id` を使います。

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

- `kind_u8`
  - `0x01`: `EthSigned`
  - `0x02`: `IcSynthetic`
- 実装: `crates/evm-core/src/hash.rs` の `stored_tx_id`

## 2. Route A: EthSigned（raw Ethereum tx）

### 2.1 内部 `tx_id`
- `submit_tx` では `stored_tx_id(kind=EthSigned, raw, None, None, None)` を採用。
- つまり内部 `tx_id` は **Ethereum tx hash（`keccak(raw)`）と同一ではない**。

### 2.2 Ethereum 互換 hash との対応
- `eth_tx_hash = keccak256(raw_tx_bytes)` は別に算出。
- `eth_tx_hash_index` に `eth_tx_hash -> tx_id` を保存。
- `eth_getTransactionByHash` 相当はこの index を引いて内部 `tx_id` に解決する。

## 3. Route B: IcSynthetic（canister 呼び出し由来）

### 3.1 内部 `tx_id`
- `stored_tx_id(kind=IcSynthetic, raw, caller_evm, canister_id, caller_principal)`。
- `caller_evm` は `caller_principal` から導出。

### 3.2 nonce の扱い
- nonce は canister 側の自動採番ではない。
- `submit_ic_tx` で `tx_bytes` ヘッダを decode し、ヘッダ内 `nonce` を使う。

## 4. ハッシュ規則（現行）

### 4.1 `tx_list_hash`

```text
tx_list_hash = keccak256(0x00 || tx_id_0 || tx_id_1 || ...)
```

- 先頭に `0x00` を付ける（domain separation）。

### 4.2 `block_hash`

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

- 先頭に `0x01` を付ける（domain separation）。

## 5. stable schema（実装の要点）

現行は「最小構成」より拡張されています。主要ポイントのみ記載します。

- ルート: `StableState`（`StableBTreeMap` 群 + `StableCell` 群）
- 主要 map:
  - `queue: seq(u64) -> tx_id`
  - `tx_store: tx_id -> StoredTxBytes`
  - `tx_locs/tx_locs_v3: tx_id -> TxLoc`
  - `blocks: block_number -> BlobPtr`
  - `receipts: tx_id -> BlobPtr`
  - `eth_tx_hash_index: eth_tx_hash(TxId wrapper) -> tx_id`
- 主要 cell:
  - `chain_state`（chain設定・base fee・`next_queue_seq` 等）
  - `head`
  - `queue_meta`

## 6. API 影響（現行動作）

- `submit_raw_tx` / `submit_tx` は内部 `tx_id` を返す。
- `rpc_eth_get_transaction_by_hash` は `eth_tx_hash_index` で解決する。
- `get_pending` / `get_receipt` は内部 `tx_id` 基準で参照する。

## 7. 実装参照

- `crates/evm-core/src/hash.rs`
- `crates/evm-core/src/chain.rs`
- `crates/ic-evm-rpc/src/lib.rs`
- `crates/evm-db/src/stable_state.rs`
- `crates/evm-db/src/chain_data/tx.rs`

