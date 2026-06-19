# specgen PR report

- base: origin/main
- base_source: manual
- check_result: pass

## verified ICP precompile targets
- compact_icp_query_input_safe_raw-8605da94
- icp_query_update_kind_rejected_raw-b2b79d8e
- icp_query_gas_observation_safe_raw-9b7ab62f
- icp_precompile_allowlist_entry_safe_raw-0ba30703
- icp_query_execution_gate_safe_raw-c8c66378
- icp_update_status_consumes_capacity_raw-882a4379
- icp_update_capacity_accepts_raw-9d22db3f

## verification
- `scripts/verify-verus.sh`: 179 verified, 0 errors
- `specgen status --check`: required by `scripts/check_icp_query_precompile_verification.sh` for each target above
- Rust/PBT evidence is tracked in each accepted JSON entry
