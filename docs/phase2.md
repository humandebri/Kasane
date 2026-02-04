# Phase2 Spec + 実装計画（Gateway前提のJSON-RPCノード）

## Phase2の目的

* Ethereum風 JSON-RPC 2.0 で **最低限の互換**を提供
* viem / ethers / foundry が「最低限」動く
* **Gateway必須**で canister は Candid API のみ

## 非目的

* mempool / pending / eth_subscribe（WS）
* eth_getLogs / filter（ログインデックス沼）
* OP互換やL1投稿（Phase3以降）

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

---

## 8) Phase2 合格条件

* eth_chainId / eth_blockNumber が返る
* eth_sendRawTransaction → eth_getTransactionReceipt が通る
* eth_call が動く（state変化なし）
* 同一stateなら同一レスポンス（決定性）

---