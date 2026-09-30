#!/usr/bin/env bash
# Build the official checker with the composite-query classification fix.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION=0.11.1
SHA256=62beb934ad93d6cab235018e6b2ad15c72a54cd2148885f7b83dc8f5362806b7
TOOL_ROOT="${REPO_ROOT}/target/tools/ic-wasm-endpoint-checker"
ARCHIVE="${TOOL_ROOT}/ic-wasm-${VERSION}.crate"
SOURCE="${TOOL_ROOT}/ic-wasm-${VERSION}"
mkdir -p "${TOOL_ROOT}"
if [[ ! -f "${ARCHIVE}" ]]; then
  curl --retry 3 -fsSL "https://static.crates.io/crates/ic-wasm/ic-wasm-${VERSION}.crate" -o "${ARCHIVE}"
fi
printf '%s  %s\n' "${SHA256}" "${ARCHIVE}" | shasum -a 256 -c - >&2
# Re-extract to make the applied source independent of previous local edits.
tar -xzf "${ARCHIVE}" -C "${TOOL_ROOT}"
patch -d "${SOURCE}" -p1 < "${REPO_ROOT}/scripts/patches/ic-wasm-0.11.1-composite-query.patch" >&2
printf '\n[workspace]\n' >> "${SOURCE}/Cargo.toml"
CARGO_TARGET_DIR="${TOOL_ROOT}/build" cargo build --locked --release \
  --manifest-path "${SOURCE}/Cargo.toml" --bin ic-wasm >&2
printf '%s\n' "${TOOL_ROOT}/build/release/ic-wasm"
