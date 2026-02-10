# RPC Gateway (Phase2)

Gateway前提で canister Candid API を Ethereum風 JSON-RPC 2.0 に変換する実装です。

## セットアップ

```bash
cd tools/rpc-gateway
npm install
cp .env.example .env.local
```

`.env.local` で最低限 `EVM_CANISTER_ID` を設定してください。

## 起動

```bash
npm run dev
```

既定: `http://127.0.0.1:8545`

## 対応メソッド

- `web3_clientVersion`
- `net_version`
- `eth_chainId`
- `eth_blockNumber`
- `eth_syncing`
- `eth_getBlockByNumber`
- `eth_getTransactionByHash`
- `eth_getTransactionReceipt`
- `eth_getBalance` (`latest` のみ)
- `eth_getCode` (`latest` のみ)
- `eth_getStorageAt` (`latest` のみ)
- `eth_call(callObject, blockTag)` (`latest` のみ)
- `eth_estimateGas(callObject, blockTag)` (`latest` のみ)
- `eth_sendRawTransaction`

## callObject 対応範囲（Phase2.2）

- サポート: `to`, `from`, `gas`, `gasPrice`, `value`, `data`, `nonce`, `maxFeePerGas`, `maxPriorityFeePerGas`, `chainId`, `type`, `accessList`
- `type` は `0x0` / `0x2` のみ受理
- `accessList` は EIP-2930 形式（`address`, `storageKeys[]`）を受理
- `nonce` 省略時は canister 側で `from` アカウントの現在 nonce を既定利用
- 未対応フィールドは `-32602 invalid params`
- バリデーション:
  - `gasPrice` と `maxFeePerGas` / `maxPriorityFeePerGas` の併用は禁止
  - `maxPriorityFeePerGas` 指定時は `maxFeePerGas` 必須
  - `maxPriorityFeePerGas <= maxFeePerGas`
  - `type=0` と `max*` は併用禁止
  - `type=2` と `gasPrice` は併用禁止

## 互換ノート

- `eth_getStorageAt` の `slot` は `QUANTITY`（例: `0x0`）と `DATA(32bytes)` の両方を受理します。
- 入力不正は `-32602 invalid params` を返します（hex不正/長さ不正/callObject不整合を含む）。
- `eth_call` の revert は `error.code = -32000` で、`error.data` に hex 文字列（`0x...`）を返します。
- canister `Err` は `RpcErrorView { code, message }` の構造化形式です。
  - `1000-1999` は入力不正として `-32602`
  - `2000+` は実行失敗として `-32000`
- `RpcErrorView.code` 固定値（Phase2.2）:
  - `1001`: Invalid params（長さ不正、fee/type/chainId不整合など）
  - `2001`: Execution failed（EVM実行失敗）
  - `1000-1999`: 入力不正予約帯
  - `2000-2999`: 実行失敗予約帯
- canister 側は分離方針に合わせて `wrapper` を薄い委譲層にし、RPC実装は `ic-evm-rpc` 側に集約しています。

## 制限値（env）

- `RPC_GATEWAY_MAX_HTTP_BODY_SIZE` (default: 262144)
- `RPC_GATEWAY_MAX_BATCH_LEN` (default: 20)
- `RPC_GATEWAY_MAX_JSON_DEPTH` (default: 20)

## 検証

```bash
npm run test
npm run lint
npm run build
```

実接続スモーク（任意）:

```bash
npm run smoke:all
```
