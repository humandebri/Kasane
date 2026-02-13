# Mainnet Method Test Report (20260213-101806)

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

| C000 | 2026-02-13T01:18:08Z | suite_start | - | event | 2028376329127 | 2028376329127 | 0 | execution start |
| query_matrix_baseline | query | ok=29/29 | agent.query baseline completed |
| C001 | 2026-02-13T01:18:23Z | set_auto_mine | adc6b0902319 | ok:variant_ok | 2028373835055 | 2028364894807 | 8940248 | baseline setup |
| set_auto_mine | update | ok:variant_ok | (variant { 17_724 }) |
| C002 | 2026-02-13T01:18:28Z | set_pruning_enabled | adc6b0902319 | ok:variant_ok | 2028362400735 | 2028353446081 | 8954654 | baseline setup |
| set_pruning_enabled | update | ok:variant_ok | (variant { 17_724 }) |
| C003 | 2026-02-13T01:18:45Z | submit_ic_tx | 3812c2f28f15 | ok:variant_ok | 2028350952009 | 2028340536218 | 10415791 | write path A |
| submit_ic_tx | update | ok:variant_ok | (   variant {     17_724 = blob "\7a\d7\b2\fe\89\0d\d4\9b\8a\5f\b9\d9\f0\b7\57\41\ba\50\43\61\8d\c9\18\c9\77\58\d7\01\7e\c6\55\38"   }, ) |
| C004 | 2026-02-13T01:18:52Z | produce_block | 8cb3baa439d3 | ok:variant_ok | 2028338592218 | 2028326514270 | 12077948 | write path A block production |
| produce_block | update | ok:variant_ok | (   variant {     17_724 = variant {       2_812_479_844 = record {         5_795_439 = 1 : nat32;         115_942_400 = 0 : nat32;         1_959_401_147 = 2 : nat64;         3_233_224_291 = 24_000 : nat64;       }     } |
| C005 | 2026-02-13T01:19:24Z | submit_ic_tx | 55850e4af2ad | ok:variant_ok | 2028307551875 | 2028297657330 | 9894545 | auto-fund test ETH key 97877977f9bcc77ed2132db8ce6960e8c4589a1e |
| submit_ic_tx | update | ok:variant_ok | (   variant {     17_724 = blob "\96\e3\82\48\e1\55\9f\e6\f1\1e\fe\18\94\4f\7a\84\ca\3f\c4\2b\b0\a7\64\8f\86\a8\02\7c\ef\30\4d\bd"   }, ) |
| C006 | 2026-02-13T01:19:31Z | produce_block | 8cb3baa439d3 | ok:variant_ok | 2028295163258 | 2028282525745 | 12637513 | auto-fund block production |
| produce_block | update | ok:variant_ok | (   variant {     17_724 = variant {       2_812_479_844 = record {         5_795_439 = 1 : nat32;         115_942_400 = 0 : nat32;         1_959_401_147 = 3 : nat64;         3_233_224_291 = 21_000 : nat64;       }     } |
| auto_fund_test_key | update | ok | funded test sender=97877977f9bcc77ed2132db8ce6960e8c4589a1e amount=100000000000000000 caller_balance_before=999906952000000000 |
| C007 | 2026-02-13T01:19:50Z | rpc_eth_send_raw_transaction | 469a54505f60 | ok:variant_ok | 2028280031673 | 2028256267593 | 23764080 | write path B |
| rpc_eth_send_raw_transaction | update | ok:variant_ok | (   variant {     17_724 = blob "\c9\2f\26\f2\dd\17\1b\83\64\0c\cf\3c\50\dc\5d\c7\8a\f3\af\e4\df\d7\cb\6b\8c\f4\2f\27\ce\1e\de\63"   }, ) |
| C008 | 2026-02-13T01:19:56Z | produce_block | 8cb3baa439d3 | ok:variant_ok | 2028254323593 | 2028244629630 | 9693963 | write path B block production |
| produce_block | update | ok:variant_ok | (   variant {     17_724 = variant {       870_523_874 = record { 4_238_151_620 = variant { 2_587_382_767 } }     }   }, ) |
| C009 | 2026-02-13T01:20:01Z | set_auto_mine | adc6b0902319 | ok:variant_ok | 2028242135558 | 2028217816847 | 24318711 | finalize: enforce disabled |
| set_auto_mine | update | ok:variant_ok | (variant { 17_724 }) |
| C010 | 2026-02-13T01:20:07Z | set_pruning_enabled | adc6b0902319 | ok:variant_ok | 2028215872847 | 2028206368560 | 9504287 | finalize: enforce disabled |
| set_pruning_enabled | update | ok:variant_ok | (variant { 17_724 }) |
| C011 | 2026-02-13T01:20:12Z | set_block_gas_limit | 6cb620903763 | ok:variant_ok | 2028204424560 | 2028195460615 | 8963945 | finalize: restore gas |
| set_block_gas_limit | update | ok:variant_ok | (variant { 17_724 }) |
| C012 | 2026-02-13T01:20:18Z | set_instruction_soft_limit | 327c4c65e42a | ok:variant_ok | 2028193516615 | 2028183986424 | 9530191 | finalize: restore instruction |
| set_instruction_soft_limit | update | ok:variant_ok | (variant { 17_724 }) |
| C013 | 2026-02-13T01:20:24Z | set_log_filter | 2874d7800fec | ok:variant_ok | 2028182042424 | 2028173085458 | 8956966 | finalize: clear log filter |
| set_log_filter | update | ok:variant_ok | (variant { 17_724 }) |
| C014 | 2026-02-13T01:20:29Z | set_miner_allowlist | ecb7e74ede7f | ok:variant_ok | 2028171141458 | 2028161619742 | 9521716 | finalize: restore allowlist |
| set_miner_allowlist | update | ok:variant_ok | (variant { 17_724 }) |
| C015 | 2026-02-13T01:20:31Z | suite_end | - | event | 2028159675742 | 2028159675742 | 0 | finalize completed |
