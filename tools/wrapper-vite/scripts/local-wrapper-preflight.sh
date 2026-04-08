#!/usr/bin/env bash
set -euo pipefail

# どこで: wrapper-vite local Juno preflight / 何を: env 整合確認後に wrapper-vite の最低限の checks を順に実行 / なぜ: Juno emulator 前提の現行 frontend 検証を 1 コマンドにするため

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WRAPPER_VITE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${WRAPPER_VITE_DIR}"
node ./scripts/local-wrapper-smoke.mjs

echo ""
echo "running preflight checks..."
npm test
npm run lint
npm run build

npm run juno:functions:build
