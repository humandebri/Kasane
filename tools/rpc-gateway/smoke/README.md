# RPC Gateway Smoke

Japanese version: [./README.ja.md](./README.ja.md)

Live-connection smoke checks for gateway. If `EVM_RPC_URL` is not set, `http://127.0.0.1:8545` is used.
Before running, ensure target RPC (gateway or upstream node) is up.

```bash
cd tools/rpc-gateway
npm run smoke:all
```

Run individual suites:

```bash
npm run smoke:viem
npm run smoke:ethers
npm run smoke:foundry
```

Policy:
- If `viem/ethers` are not installed, result is `SKIP`
- If `cast` is not installed, result is `SKIP`
- `SKIP` exits with code 0
- If `cast` execution fails, result is `FAIL` (exit code 1)
- `viem/ethers` run `eth_call` revert probe (`data: 0xfe`) and verify `error.data` returns as `0x...`
- `smoke:watch-receipt` polls `eth_getTransactionReceipt` and exits with code 1 when `status!=0x1`
