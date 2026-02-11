# Blockscout Deployment (Docker Compose)

公開向けExplorerとして Blockscout を `tools/rpc-gateway` に接続して起動する最小構成です。

## 使い方

```bash
cd deploy/blockscout
cp .env.example .env
docker compose up -d
```

既定では `http://localhost:4000` で公開されます。

## 前提

- `tools/rpc-gateway` が `http://127.0.0.1:8545` で稼働していること
- `CHAIN_ID` が canister 側の chain id と一致していること

## 検証チェック

1. `eth_blockNumber` が Blockscout UI で更新される  
2. 既知の block を開いて tx 一覧が表示される  
3. tx hash で `eth_getTransactionByHash` / `eth_getTransactionReceipt` 相当の情報が表示される  
4. エラー時は `docker compose logs blockscout` と `tools/rpc-gateway` ログで `CHAIN_ID` と RPC到達性を確認する

補助コマンド（手動）:

```bash
curl -sS -X POST http://127.0.0.1:8545 \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}'

curl -sS -X POST http://127.0.0.1:8545 \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"eth_getBlockByNumber","params":["latest",false]}'

curl -fsS http://127.0.0.1:4000 >/dev/null
```

## 役割分担

- Blockscout: 外部公開向けExplorer
- tools/explorer: 内部運用向け分析/調査UI
