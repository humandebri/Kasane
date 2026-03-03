#!/usr/bin/env bash
# where: smoke runner
# what: run viem/ethers/foundry in order
# why: unify the compatibility verification sequence
set -euo pipefail

npm run smoke:viem
npm run smoke:ethers
npm run smoke:foundry
