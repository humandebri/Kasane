# Smoke tests (optional)

Gateway の実接続スモークです。`EVM_RPC_URL` が未指定なら `http://127.0.0.1:8545` を使います。
実行前に、対象RPC（Gatewayまたは上流ノード）が起動していることを確認してください。

```bash
cd tools/rpc-gateway
npm run smoke:all
```

個別実行:

```bash
npm run smoke:viem
npm run smoke:ethers
npm run smoke:foundry
```

ポリシー:
- `viem/ethers` 未導入時は `SKIP`
- `cast` 未導入時は `SKIP`
- SKIP は終了コード 0
- `cast` 実行に失敗した場合は `FAIL`（終了コード 1）
- `viem/ethers` は `eth_call` の revert プローブ（`data: 0xfe`）を実行し、`error.data` が `0x...` で返ることを確認
