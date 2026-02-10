# Phase2 Spec + 実装計画（Gateway前提のJSON-RPCノード）

## Phase2の目的

* Ethereum風 JSON-RPC 2.0 で **最低限の互換**を提供
* viem / ethers / foundry が「最低限」動く
* **Gateway必須**で canister は Candid API のみ

## 非目的

* mempool / pending / eth_subscribe（WS）
* eth_getLogs / filter（ログインデックス沼）
* 特定L2仕様への互換追従やL1投稿（Phase3以降）

---

## 1) HTTPインタフェース（Gateway必須）

### 1.1 方針（必須）

* **Phase2では HTTP の受け口は Gateway が必須**
* canister は **HTTPを直受けしない**

理由:

* canister設計を単純化するため
* ICの http_request 機能が実験的であるため

### 1.2 canisterのエントリポイント

* canister は **Candid APIのみ**を提供
* JSON-RPC/HTTP は **Gateway が受け持つ**

### 1.3 Gateway の役割（必須）

* HTTP/JSON-RPC の受付
* リクエストの検証・制限（bodyサイズ, rate等）
* canister への Candid 呼び出しに変換
* レスポンスの JSON-RPC 整形

---

## 2) JSON-RPC 形式（互換の地雷を避ける）

### 2.1 リクエスト

* 単発: `{jsonrpc:"2.0", id, method, params}`
* batch: `[{...},{...}]`（Phase2で対応推奨）

### 2.2 レスポンス

* 成功: `{jsonrpc:"2.0", id, result}`
* 失敗: `{jsonrpc:"2.0", id, error:{code,message,data?}}`

### 2.3 エラーコード（固定）

* -32700 parse error
* -32600 invalid request
* -32601 method not found
* -32602 invalid params
* -32603 internal error
* -32000〜 ノード固有（例: queue full / tx too large）

### 2.4 Error Contract（Normative）

この節は実装ではなく**契約**として扱う。実装変更時は本節とテストを同時更新すること。

#### 2.4.1 MUST / SHOULD

* MUST: Gateway は入力不正を `-32602 invalid params` で返す。
* MUST: Gateway は想定外例外のみ `-32603 internal error` で返す。
* MUST: `eth_call` の revert は `-32000` かつ `error.data` に `0x...` を返す。
* MUST: canister `rpc_eth_call_object` / `rpc_eth_estimate_gas_object` の `Err` は `RpcErrorView { code, message }` を返す。
* MUST: Gateway は `RpcErrorView.code` の帯域で JSON-RPC code を決定する（message 文字列で機械判定しない）。
* SHOULD: `message` は人間可読向けとし、クライアント互換判定に使わない。
* SHOULD: 追加コードは予約帯に従い、既存コードの意味を変更しない。

#### 2.4.2 `RpcErrorView.code` と JSON-RPC の対応

| `RpcErrorView.code` | 意味 | GatewayのJSON-RPC `error.code` |
|---|---|---|
| `1001` | Invalid params（長さ不正、fee/type/chainId不整合など） | `-32602` |
| `2001` | Execution failed（EVM実行失敗） | `-32000` |
| `1000-1999` | 入力不正予約帯 | `-32602` |
| `2000-2999` | 実行失敗予約帯 | `-32000` |

#### 2.4.3 安定性ポリシー

* `RpcErrorView.code` は**後方互換対象**（意味変更禁止）。
* `RpcErrorView.message` は**非互換対象**（文言変更可）。
* クライアントは `message` を分岐条件に使ってはならない。

#### 2.4.4 変更手順（運用ルール）

* 新しい `RpcErrorView.code` を追加する場合:
  * 予約帯に従う（`1000-1999` 入力不正、`2000-2999` 実行失敗）
  * 本節の対応表を更新する
  * Gateway のマッピング実装を更新する
  * テスト対応表にケースを追加する

#### 2.4.5 テスト対応表

| 契約項目 | テスト/検証箇所 |
|---|---|
| `1001` が入力不正に使われる | `crates/ic-evm-rpc/tests/rpc_runtime_paths.rs` |
| `nonce` 未指定時に sender nonce を使う | `crates/ic-evm-rpc/tests/rpc_runtime_paths.rs` |
| Gateway が code帯で `-32602/-32000` を分岐 | `tools/rpc-gateway/tests/run.ts` |
| `eth_call` revert の `error.data` 形式 | `tools/rpc-gateway/tests/run.ts` |

---

## 3) Hexエンコード規約（ここを間違えるとクライアントが死ぬ）

* DATA（bytes）: `0x` + even-length hex（空は `0x`）
* QUANTITY（数値）: `0x0` か `0x` + 先頭ゼロなし（ただし0だけ例外）
* アドレス: `0x` + 40 hex（小文字で統一推奨）

---

## 4) 実装するRPC（Phase2で“ノード”になる最小集合）

### 4.1 基本（query）

* web3_clientVersion（固定文字列でOK）
* net_version（chain_idを文字列化でもOK）
* eth_chainId（Phase1の固定値）
* eth_blockNumber（head）
* eth_syncing: 常に false

### 4.2 ブロック/Tx（query）

* eth_getBlockByNumber(blockTag, fullTx)
  * blockTag: latest / 0x..number
  * pending は未対応（latestへ丸めでもOK）
* eth_getTransactionByHash(txHash)
* eth_getTransactionReceipt(txHash)

※ Phase1で tx_store / tx_index / receipts / blocks が揃ってる前提。

### 4.3 State（query）

* eth_getBalance(address, blockTag)
* eth_getCode(address, blockTag)
* eth_getStorageAt(address, slot, blockTag)

Phase2では **latestのみ厳格対応**。
過去ブロック指定は unsupported でも可（クライアントによっては要注意）。

### 4.4 実行系（query）

* eth_call(callObject, blockTag)
  * Phase1の eth_call_like（overlay REVM）に直結
* eth_estimateGas(callObject, blockTag)
  * 一度overlayで実行し gas_used を返す
  * revert は error.data に revert_data を入れる（可能なら）

#### 4.4.1 callObject 拡張（Phase2.2）

* 対応フィールド:
  * `to`, `from`, `gas`, `gasPrice`, `value`, `data`
  * `nonce`, `maxFeePerGas`, `maxPriorityFeePerGas`, `accessList`, `chainId`, `type`
* `type` は `0` / `2` のみ対応（`1` は非対応）
* strict validation:
  * `gasPrice` と `maxFeePerGas`/`maxPriorityFeePerGas` の併用禁止
  * `maxPriorityFeePerGas` 指定時は `maxFeePerGas` 必須
  * `maxPriorityFeePerGas <= maxFeePerGas`
  * `type=0` のとき `max*` 禁止
  * `type=2` のとき `gasPrice` 禁止
  * `chainId` 指定時は canister `CHAIN_ID` 一致必須
* `nonce` 省略時の既定:
  * canister 側で `from` アカウントの current nonce を採用（未存在は `0`）
* エラー方針:
  * 入力不正は `-32602 invalid params`
  * 実行失敗/revert は `-32000`（revert は `error.data` に `0x...`）
  * canister `Err` は `RpcErrorView { code, message }` を返し、gatewayで code帯により分類
  * `RpcErrorView.code` 固定値（Phase2.2）:
    * `1001`: Invalid params
    * `2001`: Execution failed
    * `1000-1999`: 入力不正予約帯
    * `2000-2999`: 実行失敗予約帯

### 4.5 送信系（update）

* eth_sendRawTransaction(rawTx)

実装モードは2つ（Phase2で決め打ち）:

**モードA（推奨: UX最強）**

* Gateway→canister update で **即ブロック化**
* 返すのは txHash（tx_id）

**モードB（スループット寄り）**

* submit_eth_tx で enqueue のみ
* 別途 evm_produceBlock（独自RPC）で確定

POCは **モードA** が簡単で強い。

---

## 5) 返すデータの最低限仕様（固定）

**Block object（最小）**

* number（QUANTITY）
* hash（block_hash）
* parentHash
* timestamp（Phase1の決定的timestamp）
* transactions（hashes or objects）
* stateRoot
* gasLimit / gasUsed（固定でも可）
* baseFeePerGas（固定でも可: 0）

**Tx object（fullTx=true）**

* hash
* from
* to
* nonce
* input
* value
* blockNumber
* transactionIndex

**Receipt object**

* transactionHash
* blockNumber
* transactionIndex
* status
* gasUsed
* contractAddress（create時のみ）
* logs: []

### 5.1 pruning 状態（Phase1.4準拠・追記）

* `policy.max_ops_per_tick`  
  * 1tick あたりの上限 ops。自動 prune の暴走防止に使う
* `oldest_kept_timestamp`  
  * `oldest_kept_block` の timestamp をキャッシュ（should_prune を O(1) にする）

---

## 6) DoS/制限（Gateway側で必須）

* max_http_body_size（例: 256KB）
* max_batch_len（例: 20）
* max_json_depth（深い入れ子拒否）
* eth_call の max_gas（固定）
* sendRawTx の max_tx_size（Phase1と同じ）
* CORS: 必要最低限

---

## 7) 実装タスク分解（チケット粒度）

### 7.1 Gateway骨格

* HTTP/JSON-RPC 受付
* CORS
* バリデーション/制限（body, depth, batch）
* canister Candid 呼び出しへの変換

### 7.2 JSON-RPCコア

* JSON parse（単発/batch）
* request validation（jsonrpc/id/method）
* error整形（コード固定）
* response serialize

### 7.3 RPCハンドラ（Gateway→canister）

* chainId / blockNumber / clientVersion / syncing
* getBlockByNumber
* getTxByHash
* getReceipt
* getBalance / getCode / getStorageAt
* eth_call / estimateGas

### 7.4 送信系（Gateway→canister update）

* sendRawTransaction（モードA or B）
* 必要なら独自 evm_produceBlock

### 7.5 互換テスト

* viem: publicClient.getBlockNumber 等
* ethers: provider.getBlockNumber 等
* foundry: 最低限のcall/send
* Rust実行コマンド（doctest除外で安定運用）:
  * `cargo test -p ic-evm-rpc --lib --tests`

---

## 8) Phase2 合格条件

* eth_chainId / eth_blockNumber が返る
* eth_sendRawTransaction → eth_getTransactionReceipt が通る
* eth_call が動く（state変化なし）
* 同一stateなら同一レスポンス（決定性）

---
