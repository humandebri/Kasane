# ChainState 72->88 Migration Runbook

Japanese version: [./chain-state-migration.ja.md](./chain-state-migration.ja.md)

## Purpose
- Apply the `ChainState` wire size change (72 -> 88) safely without backward-compatibility shims.
- Prevent accidental direct upgrades from silently resetting operational state to defaults.

## Scope Decision
- This runbook is required for releases that migrate `ChainState` from the old 72-byte wire format to the new 88-byte format.
- Use the release note marker `non-backward-compatible ChainState format change` as the trigger.

## Required Preparation
1. Reserve a maintenance window.
2. Stop the target canister.
3. Create a snapshot and record the snapshot ID.

```bash
icp canister stop -e ic <canister_id>
icp canister snapshot create -e ic <canister_id>
```

4. Export the minimum required operational data:
- latest block reference (`tip`)
- pending transactions to be replayed
- operational parameters (`base_fee`, minimum fee, mining interval, block gas limit)

## Execution Steps
1. Upgrade to the new WASM.
2. Start the canister.
3. Re-apply the operational parameters through the management API.
4. Replay pending transactions if needed.

```bash
icp canister install -e ic <canister_id> --mode upgrade --wasm <new_wasm_path>
icp canister start -e ic <canister_id>
```

## Validation
1. Check `health` for `tip_number`, `queue_len`, `block_gas_limit`, `query_instruction_soft_limit`, and `update_instruction_soft_limit`.
2. Check `get_ops_status` for `mode`, `needs_migration`, `block_gas_limit`, `query_instruction_soft_limit`, and `update_instruction_soft_limit`.
3. Submit one small transaction and confirm that auto-mining succeeds.
4. Read the receipt and confirm `gas_used` is non-zero.

## Rollback
Stop immediately and restore from the snapshot.

```bash
icp canister stop -e ic <canister_id>
icp canister snapshot restore -e ic <canister_id> <snapshot_id>
icp canister install -e ic <canister_id> --mode reinstall --wasm <old_wasm_path>
icp canister start -e ic <canister_id>
```

## Notes
- This migration does not auto-read the old 72-byte wire format.
- Do not run this on mainnet without a snapshot.
- If defaults are restored unexpectedly, do not continue normal operations; roll back immediately.
