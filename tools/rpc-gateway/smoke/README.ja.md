# Smoke tests (optional)

English version: [./README.md](./README.md)

Gateway の実接続スモークです。`EVM_RPC_URL` が未指定なら `http://127.0.0.1:8545` を使います。
実行前に、対象RPC（Gatewayまたは上流ノード）が起動していることを確認してください。

```bash
cd tools/rpc-gateway
npm run smoke:read
```

個別実行:

```bash
npm run smoke:viem
npm run smoke:ethers
npm run smoke:foundry
npm run smoke:read
npm run smoke:all
npm run smoke:watch-receipt -- 0x<tx_hash> 120 1500
```

ポリシー:
- `smoke:read` と `smoke:all` は read-only。`eth_sendRawTransaction` は呼ばない
- `viem/ethers` 未導入時は `SKIP`
- `cast` 未導入時は `SKIP`
- SKIP は終了コード 0
- `cast` 実行に失敗した場合は `FAIL`（終了コード 1）
- `viem/ethers` は `eth_call` の revert プローブ（`data: 0xfe`）を実行し、`error.data` が `0x...` で返ることを確認
- `smoke:watch-receipt` は `eth_getTransactionReceipt` をポーリングし、`status!=0x1` を失敗として終了コード 1 を返す
- staging が本番canisterを指す場合、write検証は禁止
