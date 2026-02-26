# JSON-RPC Deviations

## TL;DR
- 実装済みメソッドは限定される。
- `eth_getLogs` は filter制約が強い。
- blockTagはメソッドごとに制約があり、`latest` 系のみのものが多い。
- `eth_sendRawTransaction` は submit API委譲で、実行成功は receipt で確認する。
- `eth_feeHistory` は対応済み（`blockCount` は number/QUANTITY/10進文字列を受理）。
- `eth_gasPrice` は `base_fee` 単体ではなく、受理条件に寄せた推定値を返す。

## メソッド別差分（要点）
- `eth_getBalance`
  - `latest` 系のみ
- `eth_getTransactionCount`
  - `latest/pending/safe/finalized` のみ
  - `earliest` や過去block nonceは非対応
- `eth_getCode`
  - `latest` 系のみ
- `eth_getStorageAt`
  - `latest` 系のみ
  - slotは QUANTITY/DATA(32 bytes) 受理
- `eth_call`, `eth_estimateGas`
  - `latest` 系のみ
  - 未対応フィールドは `-32602`
- `eth_getLogs`
  - `blockHash` 非対応
  - `address` 単一のみ
  - `topics` OR配列非対応
  - 範囲超過は `-32005`
- `eth_feeHistory`
  - `blockCount` は `number` / `QUANTITY` / 10進文字列を受理
  - `blockCount <= 256`
  - `pending` は現状 `latest` 同義
- `eth_maxPriorityFeePerGas`
  - 観測データ不足時は `-32000`（`state unavailable`）
- `eth_gasPrice`
  - `max(base_fee + max(推定priority,min_priority), min_gas_price)` を返す

## 未対応メソッド
- `eth_getBlockByHash`
- `eth_newFilter` / `eth_getFilterChanges` / `eth_uninstallFilter`
- `eth_subscribe` / `eth_unsubscribe`
- `eth_pendingTransactions`

## エラー設計
- `-32602`: invalid params
- `-32000`: state unavailable / execution failed
- `-32001`: resource not found（pruned含む）
- `-32005`: limit exceeded（logs）

## 落とし穴
- 標準ノードと同じ `eth_getLogs` filterを投げる
- `eth_sendRawTransaction` だけで成功確定扱いにする

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/handlers.ts`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/README.md`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-rpc/src/lib.rs`
