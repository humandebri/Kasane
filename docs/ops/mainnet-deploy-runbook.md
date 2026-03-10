# Mainnet Deploy Runbook (`ic`)

Japanese version: [./mainnet-deploy-runbook.ja.md](./mainnet-deploy-runbook.ja.md)

## Purpose
Runbook for mainnet deployment, post-checks, rollback, and environment-specific operations.

## Covered Operations
- preflight checks
- deployment execution
- post-deploy verification
- precompile gas-ratio preparation policy
- RPC semantics version operation
- dropped-code compatibility handling
- `exec_precheck` re-sync procedure
- Contabo file placement and systemd sync
- indexer migration re-apply and redeploy
- canister upgrade and post-upgrade checks
- rollback policy (including receipt API split releases)
- optional full method test
- handling of `caller_principal` / `canister_id`
- genesis distribution notes
- signer constant rotation policy

## Usage
Use this file as the canonical index for mainnet operation phases.
For exact command sequences and environment-specific snippets, see the Japanese version.

## Precompile Gas-Ratio Policy
- Measure precompile cost before mainnet deploy with `scripts/run_precompile_profile_e2e.sh` and `scripts/measure_precompile_ratio.sh`.
- Measurement-only APIs (`get_precompile_profile`, `clear_precompile_profile`, `profile_precompile_call`) are exposed only in builds with the `precompile-profile-admin` feature.
- The default mainnet deploy path must not ship those measurement-only APIs.
- The default mainnet deploy path ships a fixed precompile ratio `1/100`; changing it later requires a redeploy.

## Important Note
- `MODE=install` / `MODE=reinstall` requires `WRAP_CANISTER_ID`.
- The deploy scripts do not auto-resolve `wrap_canister` anymore.
- When upgrading an existing `evm_canister` from a version that did not persist this field, call `set_wrap_canister_id(<wrap_canister_principal>)` once as a controller after the upgrade.
