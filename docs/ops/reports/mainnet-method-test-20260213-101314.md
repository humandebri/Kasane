# Mainnet Method Test Report (20260213-101314)

- canister: `4c52m-aiaaa-aaaam-agwwa-cai`
- environment: `ic`
- identity: `ci-local`
- execute: `1`
- strict: `1`
- full_method_required: `0`
- profile: `safe`
- auto_fund_test_key: `1`
- auto_fund_amount_wei: `100000000000000000`

## Cycle Events

| step_id | timestamp_utc | method | args_digest | status | before_cycles | after_cycles | delta | note |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |

## Method Results

| method | category | status | summary |
| --- | --- | --- | --- |

| C000 | 2026-02-13T01:13:16Z | suite_start | - | event | 2028674887930 | 2028674887930 | 0 | execution start |
| query_matrix_baseline | query | ok=29/29 | agent.query baseline completed |
| C001 | 2026-02-13T01:13:32Z | set_auto_mine | adc6b0902319 | ok:variant_non_ok | 2028671864560 | 2028662929459 | 8935101 | baseline setup |
| set_auto_mine | update | ok:variant_non_ok | (variant { 17_724 }) |
| C002 | 2026-02-13T01:13:37Z | set_auto_mine | adc6b0902319 | ok:variant_non_ok | 2028660985459 | 2028652050637 | 8934822 | finalize: enforce disabled |
| set_auto_mine | update | ok:variant_non_ok | (variant { 17_724 }) |
| C003 | 2026-02-13T01:13:43Z | set_pruning_enabled | adc6b0902319 | ok:variant_non_ok | 2028649566952 | 2028640615110 | 8951842 | finalize: enforce disabled |
| set_pruning_enabled | update | ok:variant_non_ok | (variant { 17_724 }) |
| C004 | 2026-02-13T01:13:50Z | set_block_gas_limit | 6cb620903763 | ok:variant_non_ok | 2028638671110 | 2028629167584 | 9503526 | finalize: restore gas |
| set_block_gas_limit | update | ok:variant_non_ok | (variant { 17_724 }) |
| C005 | 2026-02-13T01:13:55Z | set_instruction_soft_limit | 327c4c65e42a | ok:variant_non_ok | 2028627223584 | 2028618245507 | 8978077 | finalize: restore instruction |
| set_instruction_soft_limit | update | ok:variant_non_ok | (variant { 17_724 }) |
| C006 | 2026-02-13T01:14:03Z | set_log_filter | 2874d7800fec | ok:variant_non_ok | 2028616301507 | 2028591435286 | 24866221 | finalize: clear log filter |
| set_log_filter | update | ok:variant_non_ok | (variant { 17_724 }) |
| C007 | 2026-02-13T01:14:08Z | set_miner_allowlist | ecb7e74ede7f | ok:variant_non_ok | 2028589491286 | 2028579978047 | 9513239 | finalize: restore allowlist |
| set_miner_allowlist | update | ok:variant_non_ok | (variant { 17_724 }) |
| C008 | 2026-02-13T01:14:10Z | suite_end | - | event | 2028578034047 | 2028578034047 | 0 | finalize completed |
