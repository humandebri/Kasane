# Changelog

## 2026-02-03

- breaking(ops): `ic-evm-wrapper` install now requires `Some(InitArgs)`; empty args and `opt none` are not supported.
- ops: added `scripts/lib_init_args.sh` and updated install scripts to always pass generated `InitArgs`.
- test: aligned rpc e2e install flow with mandatory `InitArgs` and shared caller->EVM derivation.
