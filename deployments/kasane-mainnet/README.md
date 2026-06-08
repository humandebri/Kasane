# Kasane mainnet deployment

Kasane mainnet固有の設定を置く場所。

公開OSS repo `ic-evm-stack` には入れない値:

- controller/identity前提
- canister id
- ledger/wrap設定
- Contabo/systemd配置
- deploy前後のmainnet smoke設定
- incident/runbook logs

既存の `scripts/mainnet/*` と `docs/ops/*` は本repoに残す。
