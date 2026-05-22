#!/usr/bin/env bash
# where: smoke runner
# what: run read-only JSON-RPC compatibility checks
# why: keep staging checks from mutating production canister state
set -euo pipefail

npm run smoke:viem
npm run smoke:ethers
npm run smoke:foundry
