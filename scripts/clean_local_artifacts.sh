#!/usr/bin/env bash
# where: ローカル開発環境の掃除スクリプト
# what: target/.icp/.cache など再生成可能な成果物を削除
# why: ディスク肥大化と誤コミットを防ぎ、ローカル検証を安定化するため
set -euo pipefail

ALLOW_DELETE="${ALLOW_DELETE:-0}"
INCLUDE_NODE="${INCLUDE_NODE:-0}"
TARGET_MODE="${TARGET_MODE:-debug}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

log() {
  echo "[clean-local-artifacts] $*"
}

warn() {
  echo "[clean-local-artifacts] $*" >&2
}

candidate_paths=(
  "${PROJECT_ROOT}/.icp"
  "${PROJECT_ROOT}/.cache"
)

if [[ "${TARGET_MODE}" == "debug" ]]; then
  candidate_paths+=("${PROJECT_ROOT}/target/debug")
elif [[ "${TARGET_MODE}" == "all" ]]; then
  candidate_paths+=("${PROJECT_ROOT}/target")
else
  warn "invalid TARGET_MODE=${TARGET_MODE} (use: debug|all)"
  exit 1
fi

if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  candidate_paths+=("${CARGO_TARGET_DIR}")
fi

if [[ "${INCLUDE_NODE}" == "1" ]]; then
  candidate_paths+=(
    "${PROJECT_ROOT}/tools/indexer/node_modules"
    "${PROJECT_ROOT}/tools/indexer/dist"
    "${PROJECT_ROOT}/tools/explorer/node_modules"
    "${PROJECT_ROOT}/tools/explorer/.next"
  )
fi

log "project_root=${PROJECT_ROOT}"
log "candidate paths:"
for path in "${candidate_paths[@]}"; do
  if [[ -e "${path}" ]]; then
    size="$(du -sh "${path}" 2>/dev/null | awk '{print $1}')"
    log "- ${path} (size=${size:-unknown})"
  else
    log "- ${path} (missing)"
  fi
done

if [[ "${ALLOW_DELETE}" != "1" ]]; then
  warn "preview only. set ALLOW_DELETE=1 to delete listed paths."
  warn "optional: INCLUDE_NODE=1 to also remove node_modules/.next/dist."
  warn "target cleanup mode: TARGET_MODE=debug (default) or TARGET_MODE=all"
  exit 0
fi

for path in "${candidate_paths[@]}"; do
  if [[ -e "${path}" ]]; then
    log "remove ${path}"
    rm -rf "${path}"
  fi
done

log "cleanup finished"
