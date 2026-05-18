# Precompile Profile Cleanup Memo

## Purpose

- Keep the measurement path needed to tune instruction-to-gas charging for precompiles.
- Do not expose measurement-only APIs in the production canister by default.
- Keep operational measurement in scripts and PocketIC flows where possible.

## Current State

- The canister contains the profile aggregation logic.
- Precompile execution records instruction counts and computed `extra_gas`.
- The default build keeps the fixed ratio `1/100` in code, not in canister state.
- Measurement APIs exist:
  - `get_precompile_profile`
  - `clear_precompile_profile`
  - `profile_precompile_call`
- `profile_precompile_call` is an update API for PocketIC/local measurement because query calls do not persist profile entries.

## Decision

Measurement scripts are the primary workflow, but measurement builds still need minimal canister-side code because aggregation happens inside execution. Production builds should remove or feature-gate `profile_precompile_call`.

## Recommended Shape

Always-on:
- precompile profile aggregation used by execution
- fixed extra-gas ratio (`1/100` in the default build)

Feature-gated:
- `get_precompile_profile`
- `clear_precompile_profile`
- `profile_precompile_call`

Primary scripts:
- `scripts/run_precompile_profile_e2e.sh`
- `scripts/measure_precompile_ratio.sh`

## Implementation Policy

Use a cargo feature such as `precompile-profile-admin`.

- When enabled, expose `get_precompile_profile`, `clear_precompile_profile`, and `profile_precompile_call`.
- Enable it for PocketIC/local measurement builds.
- Keep it disabled for mainnet/default builds.

If production runtime overhead must be removed entirely, add a separate feature for the aggregation core as well. That makes production re-measurement require redeploy.

## Notes

- `profile_precompile_call` is not needed in production.
- `get_precompile_profile` and `clear_precompile_profile` should be closed in production when not needed.
- Charging logic remains in production.
- Ratio decisions use the IC instruction counter, not wall-clock timing.
- `scripts/measure_precompile_ratio.sh` must stop if `clear_precompile_profile` fails before measurement.
- `get_precompile_profile` should require a controller caller even in measurement builds.
- Ratio changes are code changes followed by redeploy, not runtime API changes.

## Follow-up

1. Move `profile_precompile_call` behind the feature gate.
2. Verify DID generation per feature set.
3. Align `scripts/run_precompile_profile_e2e.sh` with feature-enabled builds.
4. Add CI coverage proving default builds do not expose measurement APIs.
