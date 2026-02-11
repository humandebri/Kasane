#!/usr/bin/env bash
# where: local indexer reset helper
# what: locate and delete indexer archive paths after canister reinstall
# why: reinstall後に古いcursor/archiveが残ってMissingDataになるのを防ぐため
set -euo pipefail

ALLOW_DELETE="${ALLOW_DELETE:-0}"
INDEXER_ARCHIVE_DIR="${INDEXER_ARCHIVE_DIR:-${ARCHIVE_DIR:-}}"

log() {
  echo "[indexer-reset] $*"
}

warn() {
  echo "[indexer-reset] $*" >&2
}

candidate_paths=()

if [[ -n "${INDEXER_ARCHIVE_DIR}" ]]; then
  candidate_paths+=("${INDEXER_ARCHIVE_DIR}")
fi

if [[ -d tools/indexer/archive ]]; then
  candidate_paths+=("tools/indexer/archive")
fi

log "candidate paths (existing only):"
for path in "${candidate_paths[@]}"; do
  if [[ -e "${path}" ]]; then
    log "- ${path}"
  fi
done

log "other possible paths (scan)"
find . -maxdepth 4 -type d \( -iname "*indexer*" -o -iname "*db*" -o -iname "*data*" -o -iname "*leveldb*" -o -iname "*rocksdb*" \) 2>/dev/null

if [[ "${ALLOW_DELETE}" != "1" ]]; then
  warn "set ALLOW_DELETE=1 to delete paths listed above"
  exit 1
fi

for path in "${candidate_paths[@]}"; do
  if [[ -e "${path}" ]]; then
    log "remove ${path}"
    rm -rf "${path}"
  fi
done

log "reset finished"
