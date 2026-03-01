# Tooling (Hardhat/Foundry/ethers/viem)

## TL;DR
- ethers/viem/foundry のスモークがリポジトリ内にある。
- まず read系 + call/estimate + receipt監視を最小セットにする。

## 手順（最小）
- viem: `tools/rpc-gateway/smoke/viem_smoke.ts`
- ethers: `tools/rpc-gateway/smoke/ethers_smoke.ts`
- foundry: `tools/rpc-gateway/smoke/foundry_smoke.sh`

## 落とし穴
- revert dataを捨てて原因調査不能になる
- tx hash保存なしで監視不能になる

## 根拠
- `tools/rpc-gateway/smoke/viem_smoke.ts`
- `tools/rpc-gateway/smoke/ethers_smoke.ts`
- `tools/rpc-gateway/smoke/foundry_smoke.sh`
