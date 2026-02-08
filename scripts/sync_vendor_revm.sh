#!/usr/bin/env bash
# where: worktree上の開発環境初期化
# what: vendor/revm を既存作業ツリーから同期
# why: worktree作成時に vendor/revm が欠落してテスト不能になる事故を防ぐ

set -euo pipefail

CURRENT_ROOT="$(git rev-parse --show-toplevel)"
SOURCE_ROOT="${1:-}"
EXPECTED_REVM_SHA="$(
  git -C "${CURRENT_ROOT}" ls-tree HEAD vendor/revm | awk '{print $3}'
)"

if [[ -z "${EXPECTED_REVM_SHA}" ]]; then
  echo "error: ${CURRENT_ROOT} の HEAD に vendor/revm gitlink がありません。" >&2
  exit 1
fi

if [[ -z "${SOURCE_ROOT}" ]]; then
  while IFS= read -r wt; do
    if [[ "${wt}" == "${CURRENT_ROOT}" ]]; then
      continue
    fi
    source_sha="$(git -C "${wt}" ls-tree HEAD vendor/revm 2>/dev/null | awk '{print $3}')"
    if [[ "${source_sha}" == "${EXPECTED_REVM_SHA}" ]] && [[ -f "${wt}/vendor/revm/Cargo.toml" ]]; then
      SOURCE_ROOT="${wt}"
      break
    fi
  done < <(git worktree list --porcelain | awk '/^worktree /{print substr($0, 10)}')
fi

if [[ -z "${SOURCE_ROOT}" ]]; then
  echo "error: 同期元 worktree を特定できませんでした。" >&2
  echo "hint: gitlink SHA ${EXPECTED_REVM_SHA} と一致するworktreeが必要です。" >&2
  echo "usage: scripts/sync_vendor_revm.sh <source_repo_path>" >&2
  exit 1
fi

if [[ ! -f "${SOURCE_ROOT}/vendor/revm/Cargo.toml" ]]; then
  echo "error: 同期元に vendor/revm がありません: ${SOURCE_ROOT}" >&2
  exit 1
fi

SOURCE_REVM_SHA="$(git -C "${SOURCE_ROOT}" ls-tree HEAD vendor/revm | awk '{print $3}')"
if [[ "${SOURCE_REVM_SHA}" != "${EXPECTED_REVM_SHA}" ]]; then
  echo "error: vendor/revm のgitlink SHAが一致しません。" >&2
  echo "expected: ${EXPECTED_REVM_SHA}" >&2
  echo "source:   ${SOURCE_REVM_SHA}" >&2
  exit 1
fi

if ! git -C "${SOURCE_ROOT}/vendor/revm" cat-file -e "${EXPECTED_REVM_SHA}^{commit}" 2>/dev/null; then
  echo "error: 同期元 vendor/revm に必要なcommitがありません: ${EXPECTED_REVM_SHA}" >&2
  exit 1
fi

DEST_REVM="${CURRENT_ROOT}/vendor/revm"
if [[ -d "${DEST_REVM}/.git" ]]; then
  git -C "${DEST_REVM}" fetch --quiet "${SOURCE_ROOT}/vendor/revm" "${EXPECTED_REVM_SHA}"
else
  rm -rf "${DEST_REVM}"
  git clone --quiet "${SOURCE_ROOT}/vendor/revm" "${DEST_REVM}"
fi
git -C "${DEST_REVM}" checkout --quiet "${EXPECTED_REVM_SHA}"

echo "synced vendor/revm"
echo "source: ${SOURCE_ROOT}/vendor/revm"
echo "dest:   ${CURRENT_ROOT}/vendor/revm"
echo "sha:    ${EXPECTED_REVM_SHA}"
