# Mainnet Method Test Report (20260213-101541)

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

| C000 | 2026-02-13T01:15:43Z | suite_start | - | event | 2028546927652 | 2028546927652 | 0 | execution start |
| query_matrix_baseline | query | ok=29/29 | agent.query baseline completed |
| C001 | 2026-02-13T01:15:57Z | set_auto_mine | adc6b0902319 | ok:variant_ok | 2028544443967 | 2028535506572 | 8937395 | baseline setup |
| set_auto_mine | update | ok:variant_ok | (variant { 17_724 }) |
| C002 | 2026-02-13T01:16:03Z | set_pruning_enabled | adc6b0902319 | ok:variant_ok | 2028518192684 | 2028508698182 | 9494502 | baseline setup |
| set_pruning_enabled | update | ok:variant_ok | (variant { 17_724 }) |
| C003 | 2026-02-13T01:16:21Z | submit_ic_tx | 90e5eae178db | ok:variant_ok | 2028506214497 | 2028495797723 | 10416774 | write path A |
| submit_ic_tx | update | ok:variant_ok | (   variant {     17_724 = blob "\9a\bc\c0\95\34\60\2b\fe\73\76\66\f7\e7\6b\39\f5\7c\16\48\f8\42\eb\e3\a5\e4\ed\6b\94\b1\f3\28\96"   }, ) |
| C003 | 2026-02-13T01:16:27Z | produce_block | 8cb3baa439d3 | ok:variant_ok | 2028493853723 | 2028481892467 | 11961256 | write path A block production |
| produce_block | update | ok:variant_ok | (   variant {     17_724 = variant {       2_812_479_844 = record {         5_795_439 = 1 : nat32;         115_942_400 = 0 : nat32;         1_959_401_147 = 1 : nat64;         3_233_224_291 = 24_000 : nat64;       }     } |
| C004 | 2026-02-13T01:16:34Z | set_auto_mine | adc6b0902319 | ok:variant_ok | 2028479948467 | 2028470460986 | 9487481 | finalize: enforce disabled |
| set_auto_mine | update | ok:variant_ok | (variant { 17_724 }) |
| C005 | 2026-02-13T01:16:40Z | set_pruning_enabled | adc6b0902319 | ok:variant_ok | 2028468516986 | 2028459563601 | 8953385 | finalize: enforce disabled |
| set_pruning_enabled | update | ok:variant_ok | (variant { 17_724 }) |
| C006 | 2026-02-13T01:16:46Z | set_block_gas_limit | 6cb620903763 | ok:variant_ok | 2028457069529 | 2028448104771 | 8964758 | finalize: restore gas |
| set_block_gas_limit | update | ok:variant_ok | (variant { 17_724 }) |
| C007 | 2026-02-13T01:16:52Z | set_instruction_soft_limit | 327c4c65e42a | ok:variant_ok | 2028446160771 | 2028437179867 | 8980904 | finalize: restore instruction |
| set_instruction_soft_limit | update | ok:variant_ok | (variant { 17_724 }) |
| C008 | 2026-02-13T01:16:57Z | set_log_filter | 2874d7800fec | ok:variant_ok | 2028434685795 | 2028425726802 | 8958993 | finalize: clear log filter |
| set_log_filter | update | ok:variant_ok | (variant { 17_724 }) |
| C009 | 2026-02-13T01:17:03Z | set_miner_allowlist | ecb7e74ede7f | ok:variant_ok | 2028423782802 | 2028398887601 | 24895201 | finalize: restore allowlist |
| set_miner_allowlist | update | ok:variant_ok | (variant { 17_724 }) |
| C010 | 2026-02-13T01:17:05Z | suite_end | - | event | 2028396943601 | 2028396943601 | 0 | finalize completed |
