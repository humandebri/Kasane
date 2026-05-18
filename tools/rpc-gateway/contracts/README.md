# Gateway API Compatibility Baseline

This directory is the canonical source for the gateway API compatibility baseline.

## Included Files
- `gateway-api-compat-baseline.did`
  - Minimal Candid API baseline (`v1`) required by gateway
- `gateway-api-compat-methods.txt`
  - Baseline method list (single source)

## Operational Rules
- For any compatibility-breaking update, change the following in the same PR:
  - `gateway-api-compat-baseline.did`
  - `gateway-api-compat-methods.txt`
  - compatibility matrix in `../README.md`
- CI guard: `scripts/check_gateway_api_compat_baseline.sh`
