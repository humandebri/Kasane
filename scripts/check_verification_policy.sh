#!/usr/bin/env bash
# where: verification policy guard
# what: require verified code or TCB update for Rust business logic changes
# why: 未証明ロジックの追加をPR時に検出するため
set -euo pipefail

base_ref="${VERIFICATION_POLICY_BASE_REF:-}"
if [[ -z "${base_ref}" && -n "${GITHUB_BASE_REF:-}" ]]; then
  base_ref="origin/${GITHUB_BASE_REF}"
fi

if [[ -z "${base_ref}" ]]; then
  echo "[verification-policy] skip: VERIFICATION_POLICY_BASE_REF is not set"
  exit 0
fi

if ! git rev-parse --verify "${base_ref}" >/dev/null 2>&1; then
  if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
    git fetch origin "${GITHUB_BASE_REF}:refs/remotes/origin/${GITHUB_BASE_REF}"
  fi
fi

if ! git rev-parse --verify "${base_ref}" >/dev/null 2>&1; then
  if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
    echo "[verification-policy] base ref not found in CI: ${base_ref}" >&2
    exit 1
  fi
  echo "[verification-policy] skip: base ref not found: ${base_ref}"
  exit 0
fi

changed="$(git diff --name-only "${base_ref}...HEAD")"
rust_changed="$(printf '%s\n' "${changed}" | grep -E '^crates/.+\.rs$' || true)"
if [[ -z "${rust_changed}" ]]; then
  echo "[verification-policy] ok: no Rust crate changes"
  exit 0
fi

business_changed="$(printf '%s\n' "${changed}" | grep -E '^crates/(evm-core|evm-db|ic-evm-ops|ic-evm-rpc|ic-evm-gateway|ic-evm-tx)/src/.+\.rs$' || true)"
if [[ -z "${business_changed}" ]]; then
  echo "[verification-policy] ok"
  exit 0
fi

verified_changed="$(printf '%s\n' "${changed}" | grep -E '^crates/verified-[^/]+/' || true)"
tcb_changed="$(printf '%s\n' "${changed}" | grep -E '^docs/verification/tcb\.md$' || true)"
if [[ -z "${verified_changed}" && -z "${tcb_changed}" ]]; then
  echo "[verification-policy] Rust crate changes require crates/verified-* or docs/verification/tcb.md update" >&2
  printf '%s\n' "${business_changed}" >&2
  exit 1
fi

proof_ref="${VERIFICATION_POLICY_PROOF_REF:-}"
if [[ -z "${proof_ref}" && -n "${GITHUB_EVENT_PATH:-}" && -f "${GITHUB_EVENT_PATH}" ]]; then
  if command -v jq >/dev/null 2>&1; then
    proof_ref="$(jq -r '.pull_request.body // ""' "${GITHUB_EVENT_PATH}")"
  fi
fi

if [[ -z "${proof_ref}" ]]; then
  if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
    echo "[verification-policy] PR body must mention verified_core::<function> or TCB-<id>" >&2
    printf '%s\n' "${business_changed}" >&2
    exit 1
  fi
  echo "[verification-policy] skip: VERIFICATION_POLICY_PROOF_REF is not set"
  exit 0
fi

if grep -Eq 'verified_core::[A-Za-z0-9_:]+|TCB-[A-Za-z0-9_-]+' <<<"${proof_ref}"; then
  echo "[verification-policy] ok"
  exit 0
fi

echo "[verification-policy] PR body must mention verified_core::<function> or TCB-<id>" >&2
printf '%s\n' "${business_changed}" >&2
exit 1
