#!/usr/bin/env bash
# どこで: CI/ローカルガード
# 何を: alloy-consensus/alloy-eips の依存流入経路を検証
# なぜ: 重い依存を ic-evm-tx 境界に封じ込め、再汚染を防ぐため
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

TARGET="wasm32-unknown-unknown"

check_direct_parents() {
  local crate="$1"
  shift
  local -a allowed=("$@")

  local tree
  if ! tree="$(cargo tree --workspace --target "$TARGET" -e normal -i "$crate" 2>/dev/null)"; then
    echo "[guard] skip: $crate is not present in normal deps for target=$TARGET"
    return 0
  fi

  local stripped
  stripped="$(printf '%s\n' "$tree" \
    | sed -E 's/\x1B\[[0-9;]*[[:alpha:]]//g')"

  local parent_lines
  parent_lines="$(printf '%s\n' "$stripped" \
    | grep -E '^[├└]── ' \
    | sed -E 's/^[├└]── ([^ ]+).*/\1/' \
    | sort -u || true)"
  local -a parents=()
  while IFS= read -r line; do
    [[ -n "$line" ]] && parents+=("$line")
  done <<< "$parent_lines"

  if [[ ${#parents[@]} -eq 0 ]]; then
    echo "[guard] failed: could not parse parents for $crate"
    echo "$stripped"
    return 1
  fi

  for parent in "${parents[@]}"; do
    local ok=0
    for allow in "${allowed[@]}"; do
      if [[ "$parent" == "$allow" ]]; then
        ok=1
        break
      fi
    done
    if [[ $ok -ne 1 ]]; then
      echo "[guard] failed: unexpected direct parent for $crate: $parent"
      echo "[guard] allowed: ${allowed[*]}"
      echo "$stripped"
      return 1
    fi
  done

  echo "[guard] pass: $crate direct parents => ${parents[*]}"
}

# alloy-consensus は tx 境界(ic-evm-tx)からのみ流入を許可
check_direct_parents alloy-consensus ic-evm-tx

# alloy-eips は alloy-consensus 経由のみを許可（tx 直参照を禁止）
check_direct_parents alloy-eips alloy-consensus
