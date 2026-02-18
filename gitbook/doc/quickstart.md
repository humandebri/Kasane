# Quickstart (Gateway + Candid)

## TL;DR
- 既定ネットワークは public testnet（`chain_id=4801360`、RPC: `https://rpc-testnet.kasane.network`）。
- 導線は2つ: Path A（Gateway JSON-RPC）/ Path B（canister Candid直呼び）。
- 送信成功判定は submit結果ではなく `eth_getTransactionReceipt.status`。
- `tx_id` と `eth_tx_hash` は別物。
- deploy & call は、既存実装で確証できる範囲と要準備項目を分けて記載する。

## できること / できないこと

### できること
- 接続確認（chain id / block number）
- ネイティブ送金（署名済みraw tx投入）
- receipt監視（成功/失敗判定）
- query系の状態参照（balance/code/storage/call/estimate）

### できないこと（現行）
- pending/mempool 前提のフロー
- WebSocket購読（`eth_subscribe`）
- `eth_getLogs` の完全互換（filter制約あり）

---

## 前提条件
- 公開RPC: `https://rpc-testnet.kasane.network`
- chain id: `4801360`
- canister id（testnet運用値）: `4c52m-aiaaa-aaaam-agwwa-cai`
- `dfx`（canister query/updateを使う場合）
- Gateway経由で送信する場合は署名済み raw tx を用意

---

## Path A: Gateway JSON-RPC

### 1) 接続確認
```bash
RPC_URL="https://rpc-testnet.kasane.network"

curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}'

curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":2,"method":"eth_blockNumber","params":[]}'
```

期待結果:
- `eth_chainId` が `0x4944d0`（10進 `4801360`）
- `eth_blockNumber` が `0x...` 形式

### 2) 送金（署名済み raw tx）
```bash
RAW_TX="0x<signed_raw_tx_hex>"

curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
  --data "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"eth_sendRawTransaction\",\"params\":[\"$RAW_TX\"]}"
```

期待結果:
- `result` に `0x...` tx hash が返る（Gatewayが `tx_id` から `eth_tx_hash` を解決）

### 3) receipt監視（成功判定）
```bash
TX_HASH="0x<tx_hash_from_send>"

curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
  --data "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"eth_getTransactionReceipt\",\"params\":[\"$TX_HASH\"]}"
```

判定:
- `status == 0x1`: 実行成功
- `status == 0x0`: 実行失敗（submit成功でも失敗しうる）
- `result == null`: まだ未確定

### 代表エラー
- `-32602 invalid params`: 引数形式不正
- `-32001 resource not found`: prune境界または対象なし
- `-32000 state unavailable`: migration/corrupt等の状態

### 落とし穴
- `eth_sendRawTransaction` 成功だけで完了扱いにする
- `tx_id` と `eth_tx_hash` を混同する

---

## Path B: canister Candid直呼び

### 1) 接続確認（query）
```bash
CANISTER_ID="4c52m-aiaaa-aaaam-agwwa-cai"
NETWORK="ic"

dfx canister call --network "$NETWORK" --query "$CANISTER_ID" rpc_eth_chain_id '( )'
dfx canister call --network "$NETWORK" --query "$CANISTER_ID" rpc_eth_block_number '( )'
```

### 2) `submit_ic_tx`（IcSynthetic）
`submit_ic_tx` は以下フォーマットの `vec nat8` を受け取る:
- `[version:1][to:20][value:32][gas_limit:8][nonce:8][max_fee_per_gas:16][max_priority_fee_per_gas:16][data_len:4][data]`

`submit_ic_tx` の実際の version 値は `2`（IcSynthetic v2）です。

`IcSynthetic` では `from` をpayloadに含めません。  
wrapper が `msg_caller()` と `canister_self()` を付与して core の `TxIn::IcSynthetic` に渡し、sender を決定します。

```bash
# 例: version=2 / to=0x...01 / value=0 / gas_limit=500000 / data=""
TX_BYTES="2; ..."

dfx canister call --network "$NETWORK" "$CANISTER_ID" submit_ic_tx "(vec { $TX_BYTES })"
```

### 2.1) `submit_ic_tx` 検証フロー（重要）
- pre-submit guard
  - anonymous拒否（`auth.anonymous_forbidden`）
  - migration/cycle状態で write拒否（`ops.write.*`）
- decode/検証
  - payloadサイズ/形式
  - sender導出失敗は `arg.principal_to_evm_derivation_failed`
  - fee条件不一致は `submit.invalid_fee`
  - nonce不一致は `submit.nonce_too_low` / `submit.nonce_gap` / `submit.nonce_conflict`
- 正常時は `tx_id` を返し、採掘は非同期（`auto-production`）

### 3) nonce取得
```bash
ADDR_BLOB='"\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00"'
dfx canister call --network "$NETWORK" --query "$CANISTER_ID" expected_nonce_by_address "(blob $ADDR_BLOB)"
```

注意:
- `expected_nonce_by_address` は 20 bytes address のみ受理
- 32 bytes（bytes32エンコードprincipalの誤投入）には明示エラーを返す

### 4) raw tx投入（EthSigned）
既存ヘルパー `eth_raw_tx` を使って raw tx bytes を作る:
```bash
CHAIN_ID=4801360
PRIVKEY="<YOUR_PRIVKEY_HEX>"
RAW_TX_BYTES=$(cargo run -q -p ic-evm-core --features local-signer-bin --bin eth_raw_tx -- \
  --mode raw \
  --privkey "$PRIVKEY" \
  --to "0000000000000000000000000000000000000001" \
  --value "0" \
  --gas-price "1000000000" \
  --gas-limit "21000" \
  --nonce "0" \
  --chain-id "$CHAIN_ID")

dfx canister call --network "$NETWORK" "$CANISTER_ID" rpc_eth_send_raw_transaction "(vec { $RAW_TX_BYTES })"
```

### 5) `tx_id` 追跡（IcSynthetic）
`submit_ic_tx` の戻り値は `eth_tx_hash` ではなく内部キー `tx_id` です。  
追跡は以下を使います。
- `get_pending(tx_id)`（Queued/Included/Dropped/Unknown）
- `get_receipt(tx_id)`（実行結果）

### 落とし穴
- queryメソッドを update呼び出しして失敗する
- address 20bytes要件を満たさない（`expected_nonce_by_address`）
- `submit_ic_tx` の `tx_id` を `eth_tx_hash` と混同する
- `submit_ic_tx` 成功だけで実行成功とみなす

---

## Deploy & Call（現時点の扱い）

### コードから確認できること
- `eth_call` / `eth_estimateGas` / `eth_sendRawTransaction` の導線は実装済み
- `to` なし tx（CREATE）を受ける型・receipt `contract_address` フィールドは存在

### 要確認（一次情報不足）
- 本リポジトリ内に「solc/forge で最小bytecodeを作って testnetへdeploy」までを確定できる単一手順は未整備
- よって E2E deployサンプルは本書では `要準備項目` として扱う

### 要準備項目
- bytecode生成手段（solc / forge / hardhat）
- 署名環境（private key / chain id一致）
- nonce/fee戦略（EIP-1559 or legacy）

---

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/README.md`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/README.md`
- `/Users/0xhude/Desktop/ICP/Kasane/docs/api/rpc_eth_send_raw_transaction_payload.md`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/evm-core/src/test_bin/eth_raw_tx.rs`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-wrapper/evm_canister.did`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/handlers.ts`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-wrapper/src/lib.rs`（`submit_ic_tx`, `expected_nonce_by_address`）
- `/Users/0xhude/Desktop/ICP/Kasane/crates/evm-core/src/chain.rs`（`TxIn::IcSynthetic`, submit検証）
