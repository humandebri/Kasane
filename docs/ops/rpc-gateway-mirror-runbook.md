# RPC Gateway Mirror Runbook

Japanese version: [./rpc-gateway-mirror-runbook.ja.md](./rpc-gateway-mirror-runbook.ja.md)

## Purpose
- keep monorepo as the canonical source
- publish `tools/rpc-gateway` to `kasane-network/rpc-gateway` as mirror
- keep mirror as distribution/reference target, not edit origin

## Prerequisites
- work from monorepo root
- authenticated with `gh`
- mirror target repo: `kasane-network/rpc-gateway`
- if using automated sync, set `KASANE_MIRROR_PAT` in `Kasane` repo secrets

## Initial Setup
1. create mirror repository
2. split subtree from `tools/rpc-gateway`
3. push split branch to mirror `main`
4. optionally delete local split branch

## Routine Update
1. merge gateway changes into monorepo `main`
2. pull latest and run `git subtree split --prefix=tools/rpc-gateway`
3. push to mirror (`--force` allowed for mirror history management)

## GitHub Actions Auto-Sync
- workflow: `.github/workflows/rpc-gateway-mirror.yml`
- trigger: push to `main` with `tools/rpc-gateway/**` changes or `workflow_dispatch`
- flow: guards -> gateway test/build -> subtree split -> mirror push (non-force, then force fallback)

## Recovery
- auth issues: re-login `gh`
- history conflict: use `--force` for mirror branch
- unexpected split: verify that changes are under `tools/rpc-gateway`

## Language Policy
- `tools/rpc-gateway/README.md` is English canonical
- Japanese supplements are `*.ja.md`

## Compatibility Guards
Run before mirroring:
```bash
scripts/check_did_sync.sh
scripts/check_gateway_api_compat_baseline.sh
scripts/check_gateway_matrix_sync.sh
cd tools/rpc-gateway && npm test && npm run build
```
