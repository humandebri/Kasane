# Mainnet Deploy Runbook (`ic`)

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

## Precompile Gas-Ratio Policy
- Measure precompile cost before mainnet deploy with `scripts/run_precompile_profile_e2e.sh` and `scripts/measure_precompile_ratio.sh`.
- Measurement-only APIs (`get_precompile_profile`, `clear_precompile_profile`, `profile_precompile_call`) are exposed only in builds with the `precompile-profile-admin` feature.
- The default mainnet deploy path must not ship those measurement-only APIs.
- The default mainnet deploy path ships a fixed precompile ratio `1/100`; changing it later requires a redeploy.

## Important Note
- `InitArgs` must include both `wrap_canister_id` and `wrap_factory_address`; code defaults are no longer used.
- `build_init_args_for_current_identity(...)` now requires both `WRAP_CANISTER_ID` and `EVM_WRAP_FACTORY` to be exported first.
- `MODE=upgrade` also passes `InitArgs` via `--args`; the same Candid payload is delivered to `post_upgrade`.

`InitArgs` fields:
- `genesis_balances`: initial EVM balance allocation used for install / reinstall. Each entry requires a 20-byte `address` and non-zero `amount`; duplicate addresses are rejected.
- `wrap_canister_id`: IC principal used as the unwrap dispatch destination. Anonymous is rejected.
- `wrap_factory_address`: 20-byte EVM factory address used by the unwrap precompile for burn / allowance checks.
- `query_instruction_soft_limit`: optional. When provided, install / upgrade overwrites the query-side soft limit; when omitted, the default or existing state remains.
- `update_instruction_soft_limit`: optional. When provided, install / upgrade overwrites the update-side soft limit; when omitted, the default or existing state remains.

Operationally, config changes are applied only through install / upgrade arguments, not through a runtime admin API.
