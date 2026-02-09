#!/usr/bin/env bash
# where: CI/local guard
# what: fail if removed legacy RPC names still exist in repo surfaces
# why: avoid accidental reintroduction after breaking API migration
set -euo pipefail

if rg -n \
  --glob '!scripts/check_legacy_rpc_removed.sh' \
  '\brpc_eth_get_transaction_by_hash\b|\brpc_eth_get_transaction_receipt\b' \
  crates docs scripts README.md; then
  echo "[guard] legacy RPC symbol still present" >&2
  exit 1
fi

echo "[guard] legacy RPC symbols removed"
