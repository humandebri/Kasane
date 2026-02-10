#!/usr/bin/env bash
# where: smoke runner
# what: viem/ethers/foundry の順で実行
# why: phase2互換の確認手順を一本化するため
set -euo pipefail

npm run smoke:viem
npm run smoke:ethers
npm run smoke:foundry
