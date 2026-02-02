#!/usr/bin/env bash
# where: local env sanity check for dfx/indexer
# what: print env variables that can poison local connectivity
# why: IC_HOST/DFX_NETWORK固定で再現性が壊れるのを防ぐため
set -euo pipefail

PATTERN="${PATTERN:-DFX|IC_HOST|REPLICA|PROVIDER|CANISTER|HOST}"

log() {
  echo "[indexer-env-check] $*"
}

log "pattern=${PATTERN}"
if ! env | rg -n "${PATTERN}"; then
  log "no matching env vars"
fi

if [[ -f .env ]]; then
  log "detected .env in repo root; review manually"
fi

if [[ -f tools/indexer/.env ]]; then
  log "detected tools/indexer/.env; review manually"
fi
