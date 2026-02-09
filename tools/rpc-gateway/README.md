# RPC Gateway (Phase2)

Gateway前提で canister Candid API を Ethereum風 JSON-RPC 2.0 に変換する最小実装です。

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
- `eth_call`（`raw tx hex` を `params[0]` か `params[0].raw` で受ける最小実装）
- `eth_estimateGas`（現状未対応。`-32004 method not supported` を返す）
- `eth_sendRawTransaction`

## 未対応

- `eth_getStorageAt`（canister側API未提供のため）
- `eth_call(callObject)` のフル互換
- `eth_estimateGas(callObject)` の実測互換

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
