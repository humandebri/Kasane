# VENDORED_FROM

- Source crate: `alloy-consensus` `1.5.2` (crates.io)
- Local source path used at import time: `~/.cargo/registry/src/index.crates.io-*/alloy-consensus-1.5.2`
- Local patch purpose: decouple `k256` from `alloy-eips/kzg-sidecar` in the default/runtime dependency path.
- Files changed:
  - `Cargo.toml` (`k256` no longer implies `eips-kzg-sidecar`)
  - `Cargo.toml.orig` (added `eips-kzg-sidecar` feature and removed always-on `kzg-sidecar` from `alloy-eips` dependency)
  - `src/lib.rs` (sidecar re-exports gated by `eips-kzg-sidecar`)
  - `src/transaction/mod.rs` (sidecar re-exports gated by `eips-kzg-sidecar`)
  - `src/transaction/envelope.rs` (EIP-7594 conversion impls gated by `eips-kzg-sidecar`)
  - `src/transaction/eip4844.rs` (added `NoEip4844Sidecar` fallback + sidecar-specific impls gated)
  - `src/transaction/eip4844_sidecar.rs` (sidecar impls gated + fallback impl for `NoEip4844Sidecar`)

## Rationale
This repository uses `alloy-consensus` for tx recovery in `ic-evm-tx` and aims to avoid unnecessary KZG sidecar coupling in the default runtime path.
Directly removing `kzg-sidecar` from dependency features breaks upstream type defaults and re-exports.
To keep behavior for sidecar-enabled builds while reducing default runtime dependency surface, this fork introduces source-level `cfg(feature = "eips-kzg-sidecar")` splits and a fallback `NoEip4844Sidecar` type for non-sidecar builds.

## Maintenance notes
- High-conflict area on future upgrade:
  - `src/transaction/eip4844.rs` (default type parameters and sidecar conversion helpers change frequently upstream)
  - `src/transaction/envelope.rs` (conversion impl blocks for 4844/7594)
- Upgrade procedure:
  - first rebase vendor crate to upstream version
  - then re-apply this fork delta (feature split + fallback type)
  - validate with:
    - `cargo check -p ic-evm-core -p ic-evm-wrapper`
    - `scripts/check_alloy_isolation.sh`
