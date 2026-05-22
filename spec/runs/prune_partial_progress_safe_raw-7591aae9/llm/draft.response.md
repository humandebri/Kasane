command: /Users/0xhude/.nvm/versions/node/v22.22.0/bin/codex exec --sandbox read-only <prompt>
exit_code: 0
timeout: false
truncated: false

## stdout
候補:

```rust
requires true

ensures result == (
    previous_present <= 1
    && next_present <= 1
    && did_work <= 1
    && stopped_for_budget <= 1
    && ops_used <= max_ops
    && (did_work == 0 || next_present == 1)
    && (previous_present == 0 || next_present == 1)
    && (did_work == 0 || previous_present == 0 || previous_boundary < next_boundary)
    && (previous_present == 0
        || next_present == 0
        || previous_boundary == next_boundary
        || did_work == 1)
    && (previous_present == 0 || next_present == 0 || previous_boundary <= next_boundary)
    && (next_present == 0 || next_boundary < next_cursor)
    && (stopped_for_budget == 0 || next_present == 1)
    && (stopped_for_budget == 0
        || max_ops < next_ops_needed
        || (next_ops_needed <= max_ops && max_ops - next_ops_needed < ops_used))
)
```

要点:
- `did_work == 1` なら次状態が存在し、既存境界がある場合は境界が前進する。
- 既存境界がある場合、次状態は消えない。
- 未作業で前後状態がある場合、境界は同一。
- 予算停止時は次状態が存在し、次操作が予算超過または残予算不足。


## stderr
Reading additional input from stdin...
2026-05-16T10:07:21.840614Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: prompt must be at most 128 characters path=/Users/0xhude/.codex/.tmp/plugins/plugins/build-ios-apps/.codex-plugin/plugin.json
2026-05-16T10:07:21.843024Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: maximum of 3 prompts is supported path=/Users/0xhude/.codex/.tmp/plugins/plugins/plugin-eval/.codex-plugin/plugin.json
2026-05-16T10:07:21.854899Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: maximum of 3 prompts is supported path=/Users/0xhude/.codex/.tmp/plugins/plugins/twilio-developer-kit/.codex-plugin/plugin.json
2026-05-16T10:07:21.855013Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: maximum of 3 prompts is supported path=/Users/0xhude/.codex/.tmp/plugins/plugins/openai-developers/.codex-plugin/plugin.json
OpenAI Codex v0.130.0
--------
workdir: /Users/0xhude/Desktop/ICP/Kasane
model: gpt-5.5
provider: openai
approval: never
sandbox: read-only
reasoning effort: high
reasoning summaries: none
session id: 019e3041-426a-7492-a3f1-34ff1f629420
--------
user
Generate a concise spec draft candidate:
pub fn prune_partial_progress_safe_raw(
    previous_present: u64,
    previous_boundary: u64,
    next_present: u64,
    next_boundary: u64,
    next_cursor: u64,
    max_ops: u64,
    ops_used: u64,
    next_ops_needed: u64,
    did_work: u64,
    stopped_for_budget: u64,
) -> bool
{
    previous_present <= 1
        && next_present <= 1
        && did_work <= 1
        && stopped_for_budget <= 1
        && ops_used <= max_ops
        && (did_work == 0 || next_present == 1)
        && (previous_present == 0 || next_present == 1)
        && (did_work == 0 || previous_present == 0 || previous_boundary < next_boundary)
        && (previous_present == 0
            || next_present == 0
            || previous_boundary == next_boundary
            || did_work == 1)
        && (previous_present == 0 || next_present == 0 || previous_boundary <= next_boundary)
        && (next_present == 0 || next_boundary < next_cursor)
        && (stopped_for_budget == 0 || next_present == 1)
        && (stopped_for_budget == 0
            || max_ops < next_ops_needed
            || (next_ops_needed <= max_ops && max_ops - next_ops_needed < ops_used))
}
2026-05-16T10:07:24.120728Z ERROR rmcp::transport::worker: worker quit with fatal: Transport channel closed, when AuthRequired(AuthRequiredError { www_authenticate_header: "Bearer realm=\"OAuth\", resource_metadata=\"https://mcp.cloudflare.com/.well-known/oauth-protected-resource/mcp\", error=\"invalid_token\", error_description=\"Missing or invalid access token\"" })
2026-05-16T10:07:35.751123Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: prompt must be at most 128 characters path=/Users/0xhude/.codex/.tmp/plugins/plugins/build-ios-apps/.codex-plugin/plugin.json
2026-05-16T10:07:35.751967Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: maximum of 3 prompts is supported path=/Users/0xhude/.codex/.tmp/plugins/plugins/plugin-eval/.codex-plugin/plugin.json
2026-05-16T10:07:35.760035Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: maximum of 3 prompts is supported path=/Users/0xhude/.codex/.tmp/plugins/plugins/twilio-developer-kit/.codex-plugin/plugin.json
2026-05-16T10:07:35.760132Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: maximum of 3 prompts is supported path=/Users/0xhude/.codex/.tmp/plugins/plugins/openai-developers/.codex-plugin/plugin.json
2026-05-16T10:07:35.797503Z  WARN codex_core_skills::loader: ignoring interface.icon_small: icon path must not contain '..'
2026-05-16T10:07:35.797518Z  WARN codex_core_skills::loader: ignoring interface.icon_large: icon path must not contain '..'
2026-05-16T10:07:35.798354Z  WARN codex_core_skills::loader: ignoring interface.icon_small: icon path must not contain '..'
2026-05-16T10:07:35.798381Z  WARN codex_core_skills::loader: ignoring interface.icon_large: icon path must not contain '..'
2026-05-16T10:07:35.799599Z  WARN codex_core_skills::loader: ignoring interface.icon_small: icon path must not contain '..'
2026-05-16T10:07:35.799613Z  WARN codex_core_skills::loader: ignoring interface.icon_large: icon path must not contain '..'
2026-05-16T10:07:35.800928Z  WARN codex_core_skills::loader: ignoring interface.icon_small: icon path must not contain '..'
2026-05-16T10:07:35.800940Z  WARN codex_core_skills::loader: ignoring interface.icon_large: icon path must not contain '..'
2026-05-16T10:07:35.802537Z  WARN codex_core_skills::loader: ignoring interface.icon_small: icon path must not contain '..'
2026-05-16T10:07:35.802549Z  WARN codex_core_skills::loader: ignoring interface.icon_large: icon path must not contain '..'
2026-05-16T10:07:35.805753Z  WARN codex_core_skills::loader: ignoring interface.icon_small: icon path must not contain '..'
2026-05-16T10:07:35.805765Z  WARN codex_core_skills::loader: ignoring interface.icon_large: icon path must not contain '..'
2026-05-16T10:07:35.921015Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: maximum of 3 prompts is supported path=/Users/0xhude/.codex/.tmp/plugins/plugins/openai-developers/.codex-plugin/plugin.json
2026-05-16T10:07:35.931666Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: maximum of 3 prompts is supported path=/Users/0xhude/.codex/.tmp/plugins/plugins/openai-developers/.codex-plugin/plugin.json
2026-05-16T10:07:35.931897Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: maximum of 3 prompts is supported path=/Users/0xhude/.codex/.tmp/plugins/plugins/openai-developers/.codex-plugin/plugin.json
2026-05-16T10:07:37.631634Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: prompt must be at most 128 characters path=/Users/0xhude/.codex/.tmp/plugins/plugins/build-ios-apps/.codex-plugin/plugin.json
2026-05-16T10:07:37.632333Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: maximum of 3 prompts is supported path=/Users/0xhude/.codex/.tmp/plugins/plugins/plugin-eval/.codex-plugin/plugin.json
2026-05-16T10:07:37.636922Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: maximum of 3 prompts is supported path=/Users/0xhude/.codex/.tmp/plugins/plugins/twilio-developer-kit/.codex-plugin/plugin.json
2026-05-16T10:07:37.636998Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: maximum of 3 prompts is supported path=/Users/0xhude/.codex/.tmp/plugins/plugins/openai-developers/.codex-plugin/plugin.json
2026-05-16T10:07:37.662686Z  WARN codex_core_skills::loader: ignoring interface.icon_small: icon path must not contain '..'
2026-05-16T10:07:37.662699Z  WARN codex_core_skills::loader: ignoring interface.icon_large: icon path must not contain '..'
2026-05-16T10:07:37.663418Z  WARN codex_core_skills::loader: ignoring interface.icon_small: icon path must not contain '..'
2026-05-16T10:07:37.663425Z  WARN codex_core_skills::loader: ignoring interface.icon_large: icon path must not contain '..'
2026-05-16T10:07:37.664150Z  WARN codex_core_skills::loader: ignoring interface.icon_small: icon path must not contain '..'
2026-05-16T10:07:37.664156Z  WARN codex_core_skills::loader: ignoring interface.icon_large: icon path must not contain '..'
2026-05-16T10:07:37.664915Z  WARN codex_core_skills::loader: ignoring interface.icon_small: icon path must not contain '..'
2026-05-16T10:07:37.664922Z  WARN codex_core_skills::loader: ignoring interface.icon_large: icon path must not contain '..'
2026-05-16T10:07:37.665615Z  WARN codex_core_skills::loader: ignoring interface.icon_small: icon path must not contain '..'
2026-05-16T10:07:37.665620Z  WARN codex_core_skills::loader: ignoring interface.icon_large: icon path must not contain '..'
2026-05-16T10:07:37.667208Z  WARN codex_core_skills::loader: ignoring interface.icon_small: icon path must not contain '..'
2026-05-16T10:07:37.667214Z  WARN codex_core_skills::loader: ignoring interface.icon_large: icon path must not contain '..'
2026-05-16T10:07:37.731898Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: maximum of 3 prompts is supported path=/Users/0xhude/.codex/.tmp/plugins/plugins/openai-developers/.codex-plugin/plugin.json
2026-05-16T10:07:37.739841Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: maximum of 3 prompts is supported path=/Users/0xhude/.codex/.tmp/plugins/plugins/openai-developers/.codex-plugin/plugin.json
2026-05-16T10:07:37.740159Z  WARN codex_core_plugins::manifest: ignoring interface.defaultPrompt: maximum of 3 prompts is supported path=/Users/0xhude/.codex/.tmp/plugins/plugins/openai-developers/.codex-plugin/plugin.json
codex
`specgen-cli` skillを使う。既存のspec表記に寄せるため、関連ファイルだけ確認する。
exec
/bin/zsh -lc "sed -n '1,220p' /Users/0xhude/.codex/skills/specgen-cli/SKILL.md && rg -n \"prune_partial|partial_progress|next_ops_needed|stopped_for_budget|requires|ensures\" ." in /Users/0xhude/Desktop/ICP/Kasane
 succeeded in 0ms:
---
name: specgen-cli
description: Use the local specgen CLI for Rust function specification, scenario review, accepted spec management, Verus contract injection, implementation verification, PR gate checks, and report generation. Trigger when users ask how to use specgen, run target/add/extract/draft/scenarios/review/spec/accept/apply-contract/gen-verus/verify/status/gate/report, diagnose specgen artifacts under spec/, or explain command order and failure gates.
---

# Specgen CLI

## Workflow

Work from the repository root that contains `Cargo.toml` and `spec/`.

1. Confirm the CLI exists: prefer `specgen <command>` when installed, or `cargo run -- <command>` when working from a source checkout without a global binary.
2. Read `references/cli.md` when command order, artifact paths, failure gates, or examples are needed.
3. Keep generated artifacts under `spec/`; do not hand-edit accepted markdown except to diagnose drift.
4. Use `status <target> --check` for one target and `gate` for PR-level CI checks.
5. For repository development, verify both gate proofs: `verus --crate-type=lib -o /tmp/specgen_verified_core.rlib proofs/verified_core_verus.rs` and `verus --crate-type=lib -o /tmp/specgen_gate_e2e.rlib proofs/gate_e2e_verus.rs`.

## Standard Flow

```bash
specgen init
specgen target add <file> <function>
specgen extract <target>
specgen draft <target>
specgen scenarios <target>
specgen review <target>
specgen scenario mark <target> <scenario-id> --status accepted --note "<reason>"
specgen spec add-pre <target> "<verus expr>"
specgen spec add-post <target> "<verus expr using result>"
specgen spec add-criterion <target> "<criterion>"
specgen spec link-test <target> <scenario-id> --command "<cmd>" --test "<name>"
specgen accept <target>
specgen apply-contract <target>
specgen gen-verus <target>
specgen verify <target>
specgen status <target> --check
```

Use `specgen run <file> <function>` only for the early pipeline through review. It does not mark scenarios, add spec terms, accept, apply contracts, generate Verus target records, or verify.

For PR-level review elimination:

```bash
specgen gate
specgen report
```

Use `--base <rev>` only when automatic base detection cannot infer the PR base from CI env, upstream, or `origin/main`.

## Review And Acceptance Rules

- Mark every scenario with `accepted`, `rejected`, or `documented`; include a non-empty `--note`.
- Add at least one precondition, postcondition, acceptance criterion, and test evidence before `accept`.
- Write pre/post as Verus expressions. Postconditions use the fixed return name `result`.
- If `reviews/arbiter.json` contains `human_questions`, answer each `question_id` in `spec/feedback/<slug>.md` with a `### <question_id>` heading before `accept`.
- `accept` writes accepted JSON/MD. The JSON is source of truth; do not hand-edit accepted markdown except to diagnose drift.
- `apply-contract` injects `requires` / `ensures` into the target function and records `contract_hash`.
- `gen-verus` and `verify` require the applied contract hash to match accepted JSON.
- `verify` writes typed JSON as source of truth and Markdown for human reading.
- Verification targets the implementation file, not a proof-only harness.
- The repository E2E-style proof covers the typed gate decision core under correct observations; it does not prove filesystem, git, provider, or process I/O.

## Supported Contract Injection

- v1 supports free functions with non-unit returns.
- The injected return name is always `result`.
- Methods, trait functions, generics, lifetimes, async, unsafe, FFI, macro dependencies, I/O, and unresolved dependencies should be treated as unsupported unless the CLI explicitly accepts them.
- The target crate is treated as Verus-specific after contract injection; normal `cargo test` compatibility is not guaranteed.

## Troubleshooting

- `scenario needs review`: mark all scenarios with a final status.
- `human questions remain`: add matching `### question_id` headings to the feedback file.
- `current dirty paths` or `semantic_hash mismatch`: commit/revert target changes or regenerate from `extract`.
- `provider failure`: inspect `spec/runs/<slug>/llm/*.response.md` for non-zero exit, timeout, or truncation.
- `contract hash mismatch`: rerun `apply-contract <target>` after changing accepted JSON.
- `verify report hash mismatch`: rerun `verify` after changing accepted JSON or applying a new contract.
- `missing verify report`: run `apply-contract`, `gen-verus`, then `verify`.
./vendor/alloy-consensus/src/crypto.rs:230:    /// Recover signer address from message hash. This ensures that the signature S value is
./scripts/profile_wasm_deps.sh:25:  --skip-build            Skip cargo build step (requires --wasm or prebuilt target artifact)
./crates/ic-evm-rpc/src/lib.rs:741:        return Err("maxPriorityFeePerGas requires maxFeePerGas".to_string());
./crates/ic-evm-rpc/tests/rpc_runtime_paths.rs:889:    assert_eq!(err.message, "maxPriorityFeePerGas requires maxFeePerGas");
./docs/ops/mainnet-deploy-runbook.md:33:- The default mainnet deploy path ships a fixed precompile ratio `1/100`; changing it later requires a redeploy.
./docs/ops/mainnet-deploy-runbook.md:37:- `build_init_args_for_current_identity(...)` now requires both `WRAP_CANISTER_ID` and `EVM_WRAP_FACTORY` to be exported first.
./docs/ops/mainnet-deploy-runbook.md:41:- `genesis_balances`: initial EVM balance allocation used for install / reinstall. Each entry requires a 20-byte `address` and non-zero `amount`; duplicate addresses are rejected.
./docs/verification/tcb.md:9:- `crates/verified-*/src`: adapterが直接呼ぶ実装関数に付けた `requires` / `ensures` / `invariant` 仕様。
./crates/evm-core/src/wrap_precompile_tests.rs:71:fn native_value_to_e8s_requires_exact_ledger_unit() {
./scripts/test_lib_init_args.sh:4:# why: gateway install/upgrade now requires explicit wrap settings
./scripts/lib_legacy_wrap_drain.sh:171:      echo "[legacy-wrap-drain] empty request id file requires ALLOW_EMPTY_LEGACY_WRAP_REQUESTS=1" >&2
./vendor/revm/book/src/dev.md:3:It requires running
./vendor/ark-relations/src/r1cs/constraint_system.rs:609:                                eprintln!("Constraint trace requires enabling `ConstraintLayer`");
./spec/reports/block_is_prunable-04224fd7_tests.json:18:      "output": "\nrunning 1 test\ntest stable_codec::tests::prune_journal_len_accounts_for_header_and_ptrs ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 30 filtered out; finished in 0.00s\n\n\nrunning 3 tests\ntest need_prune_tracks_time_and_capacity_triggers ... ok\ntest prune_cursor_transitions_are_monotonic ... ok\ntest prune_tx_ops_count_present_indexes ... ok\n\ntest result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.00s\n\n\nrunning 3 tests\ntest prune_boundary_is_monotonic_and_retention_safe ... ok\ntest prune_cleanup_complete_requires_observable_indexes_gone ... ok\ntest prune_prunable_boundary_excludes_retained_range ... ok\n\ntest result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 45 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s\n\n\nrunning 1 test\ntest export_pruned_and_oldest_exportable ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 2.73s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 15 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.00s\n\n\nrunning 3 tests\ntest prune_blocks_keeps_head_and_retain_range ... ok\ntest prune_blocks_removes_old_data ... ok\ntest prune_blocks_respects_max_ops ... ok\n\ntest result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.91s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s\n\n\nrunning 6 tests\ntest recover_without_journal_advances_next_prune_block_to_boundary ... ok\ntest quarantine_is_not_reused_during_prune ... ok\ntest prune_journal_recovery_frees_quarantine ... ok\ntest prune_journal_recovery_removes_seen_tx ... ok\ntest prune_is_idempotent ... ok\ntest recover_prune_journal_handles_orphan_entry_without_cursor ... ok\n\ntest result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 20.18s\n\n\nrunning 1 test\ntest need_prune_ignores_enabled_flag ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 4.78s\n\n\nrunning 3 tests\ntest capacity_trigger_sets_need_prune ... ok\ntest hard_emergency_prunes_down_to_one_block ... ok\ntest estimated_kept_bytes_drops_after_prune_and_can_clear_cap_trigger ... ok\n\ntest result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 12.89s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.00s\n\n\nrunning 5 tests\ntest tests::prune_boundary_for_number_returns_boundary_only_for_pruned_range ... ok\ntest tests::receipt_lookup_status_returns_possibly_pruned_when_loc_is_gone ... ok\ntest tests::get_block_returns_pruned_for_pruned_boundary ... ok\ntest tests::get_receipt_returns_pruned_when_location_is_before_boundary ... ok\ntest tests::receipt_lookup_status_returns_pruned_when_loc_indicates_included_before_boundary ... ok\n\ntest result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 109 filtered out; finished in 6.26s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s\n\n\nrunning 3 tests\ntest tests::get_receipt_returns_pruned_when_location_is_before_boundary ... ok\ntest tests::get_receipt_returns_ok_for_retained_receipt ... ok\ntest tests::get_receipt_returns_not_found_for_corrupt_receipt_payload ... ok\n\ntest result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 111 filtered out; finished in 4.33s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s\n\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.09s\n     Running unittests src/lib.rs (target/debug/deps/verified_core-19b7446cc2719b18)\n     Running tests/prune.rs (target/debug/deps/prune-e096e7d1548b4ef0)\n     Running tests/prune_safety.rs (target/debug/deps/prune_safety-8d818e529675359d)\n   Compiling evm-db v0.1.0 (/Users/0xhude/Desktop/ICP/Kasane/crates/evm-db)\n   Compiling ic-evm-tx v0.1.0 (/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-tx)\n   Compiling ic-evm-core v0.1.0 (/Users/0xhude/Desktop/ICP/Kasane/crates/evm-core)\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 22.88s\n     Running unittests src/lib.rs (target/debug/deps/evm_core-996f496f046abfe9)\n     Running unittests src/test_bin/derive_evm_address.rs (target/debug/deps/derive_evm_address-84b4a4905d5f107d)\n     Running tests/corrupt_localization.rs (target/debug/deps/corrupt_localization-52372d9090ce758c)\n     Running tests/dropped_ring.rs (target/debug/deps/dropped_ring-8332ba37ad8f084e)\n     Running tests/export_api.rs (target/debug/deps/export_api-eb6713884dd82d8d)\n     Running tests/internal_trace_execution.rs (target/debug/deps/internal_trace_execution-ebda4e1514960e48)\n     Running tests/native_deposit_credit.rs (target/debug/deps/native_deposit_credit-5fa5a6ecef7fdb8d)\n     Running tests/phase1_blockhash_window.rs (target/debug/deps/phase1_blockhash_window-18d7ae8523fadac2)\n     Running tests/phase1_chain_behavior.rs (target/debug/deps/phase1_chain_behavior-e7114b82790bb9ca)\n     Running tests/phase1_dev_mint.rs (target/debug/deps/phase1_dev_mint-aa8fa74b8a819a04)\n     Running tests/phase1_eth_decode.rs (target/debug/deps/phase1_eth_decode-d2cee4b79d2b80e8)\n     Running tests/phase1_fee_ordering.rs (target/debug/deps/phase1_fee_ordering-9a88b6c63bb0b838)\n     Running tests/phase1_fee_rules.rs (target/debug/deps/phase1_fee_rules-af8584fdf5a0e9b7)\n     Running tests/phase1_hash.rs (target/debug/deps/phase1_hash-2f4293a876c9b277)\n     Running tests/phase1_ic_decode.rs (target/debug/deps/phase1_ic_decode-ce21516bbead6ae9)\n     Running tests/phase1_nonce_sequence.rs (target/debug/deps/phase1_nonce_sequence-fddeb066e18ddb3a)\n     Running tests/phase1_produce_block_drop.rs (target/debug/deps/phase1_produce_block_drop-f9999372981b8881)\n     Running tests/phase1_prune.rs (target/debug/deps/phase1_prune-eaba7b0e7702e8b0)\n     Running tests/phase1_queue_caps.rs (target/debug/deps/phase1_queue_caps-77ff46589beb782f)\n     Running tests/phase1_revm_db.rs (target/debug/deps/phase1_revm_db-9b6315393b205584)\n     Running tests/phase1_runtime_indexes.rs (target/debug/deps/phase1_runtime_indexes-130b3af68ea5049b)\n     Running tests/phase1_storage_trap_rollback.rs (target/debug/deps/phase1_storage_trap_rollback-68b2d328b56923ef)\n     Running tests/phase1_tx_in.rs (target/debug/deps/phase1_tx_in-935bac7fcdc324c4)\n     Running tests/pr0_snapshots.rs (target/debug/deps/pr0_snapshots-29b1bfb8b438b250)\n     Running tests/prune_journal.rs (target/debug/deps/prune_journal-4e717754b53b56e0)\n     Running tests/prune_policy.rs (target/debug/deps/prune_policy-0e65c38307049f40)\n     Running tests/prune_triggers.rs (target/debug/deps/prune_triggers-f4a81fe7fb49f90d)\n     Running tests/state_root_migration.rs (target/debug/deps/state_root_migration-c534f19221567835)\n     Running tests/tx_locs_migration.rs (target/debug/deps/tx_locs_migration-e46ca7d82aa50ae2)\n     Running tests/tx_recovery_isolation.rs (target/debug/deps/tx_recovery_isolation-14feaaaddc89c5d5)\n     Running tests/wrap_precompile_query.rs (target/debug/deps/wrap_precompile_query-70cd0fe4a3f2749f)\n   Compiling evm-db v0.1.0 (/Users/0xhude/Desktop/ICP/Kasane/crates/evm-db)\n   Compiling ic-evm-tx v0.1.0 (/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-tx)\n   Compiling ic-evm-ops v0.1.0 (/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-ops)\n   Compiling ic-evm-core v0.1.0 (/Users/0xhude/Desktop/ICP/Kasane/crates/evm-core)\n   Compiling ic-evm-rpc v0.1.0 (/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-rpc)\n   Compiling ic-evm-gateway v0.1.0 (/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-gateway)\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 9.22s\n     Running unittests src/lib.rs (target/debug/deps/ic_evm_gateway-69361869154b31cf)\n     Running tests/canbench_init_args.rs (target/debug/deps/canbench_init_args-764adada704d54eb)\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.54s\n     Running unittests src/lib.rs (target/debug/deps/ic_evm_gateway-69361869154b31cf)\n     Running tests/canbench_init_args.rs (target/debug/deps/canbench_init_args-764adada704d54eb)\n"
./spec/reports/receipt_index_location_bidirectional-7f362e2c_tests.json:18:      "output": "\nrunning 3 tests\ntest no_reorg_append_only_requires_next_head_and_old_observations_unchanged ... ok\ntest receipt_index_location_bidirectional_requires_all_reverse_links ... ok\ntest staged_tx_requires_current_pending_and_live_payload ... ok\n\ntest result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n\nrunning 1 test\ntest tests::gateway_submit_ic_tx_adapter_preserves_queue_and_receipt_invariants ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 114 filtered out; finished in 1.51s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s\n\n\nrunning 1 test\ntest tests::replacement_old_tx_is_not_staged_or_included ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 114 filtered out; finished in 5.48s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s\n\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.03s\n     Running tests/p0_safety.rs (target/debug/deps/p0_safety-e40acc0737b79126)\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.11s\n     Running unittests src/lib.rs (target/debug/deps/ic_evm_gateway-69361869154b31cf)\n     Running tests/canbench_init_args.rs (target/debug/deps/canbench_init_args-764adada704d54eb)\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.16s\n     Running unittests src/lib.rs (target/debug/deps/ic_evm_gateway-69361869154b31cf)\n     Running tests/canbench_init_args.rs (target/debug/deps/canbench_init_args-764adada704d54eb)\n"
./spec/reports/submit_transition_safe_raw-3a7d7873_tests.json:14:      "command": "cargo test -p verified-core && cargo test -p ic-evm-core replacement_requires_higher_effective_fee && cargo test -p ic-evm-gateway gateway_submit_ic_tx_adapter_preserves_queue_and_receipt_invariants",
./spec/reports/submit_transition_safe_raw-3a7d7873_tests.json:18:      "output": "\nrunning 31 tests\ntest batch::tests::batch_done_matches_iterator_and_limit_state ... ok\ntest batch::tests::processed_count_saturates ... ok\ntest batch::tests::cursors_and_snapshot_capacity_are_monotonic ... ok\ntest block::tests::block_height_and_timestamp_are_monotonic ... ok\ntest block::tests::execution_stops_on_gas_or_instruction_budget ... ok\ntest block::tests::remaining_budget_and_block_gas_are_saturating ... ok\ntest block_persist::tests::persist_batch_requires_nonempty_matching_counts ... ok\ntest block_persist::tests::single_commit_requires_index_receipt_and_nonzero_block ... ok\ntest dropped_ring::tests::push_grows_until_capacity_then_evicts ... ok\ntest fee::tests::effective_gas_price_caps_priority ... ok\ntest fee::tests::effective_gas_price_rejects_invalid_bounds ... ok\ntest fee::tests::fee_accounting_uses_saturating_components ... ok\ntest fee::tests::min_fee_satisfied_checks_dynamic_and_legacy ... ok\ntest nonce::tests::bump_expected_nonce_saturates ... ok\ntest nonce::tests::classify_nonce_handles_current_and_replacement ... ok\ntest nonce::tests::classify_nonce_rejects_low_and_gap ... ok\ntest pending::tests::current_and_min_decisions_are_explicit ... ok\ntest pending::tests::decrement_count_removes_at_zero_or_one ... ok\ntest pending::tests::pending_min_after_advance_sets_or_removes ... ok\ntest pending::tests::promote_decision_tracks_min_nonce ... ok\ntest pending::tests::rebuild_pending_counts_only_meta_entries ... ok\ntest queue::tests::classify_pending_caps_eviction_requires_higher_fee ... ok\ntest queue::tests::classify_pending_caps_orders_limits ... ok\ntest queue::tests::queue_cursor_transitions_are_monotonic ... ok\ntest stable_codec::tests::fixed_len_requires_exact_length ... ok\ntest stable_codec::tests::prune_journal_len_accounts_for_header_and_ptrs ... ok\ntest stable_codec::tests::variable_items_len_checks_count_and_overflow ... ok\ntest state_diff::tests::account_commit_deletes_only_destroyed_or_empty_touched_accounts ... ok\ntest state_diff::tests::storage_and_code_commit_match_presence ... ok\ntest tx_index::tests::included_position_requires_kind_block_and_index_match ... ok\ntest tx_index::tests::next_included_index_rejects_u32_overflow ... ok\n\ntest result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n\nrunning 6 tests\ntest need_prune_tracks_time_and_capacity_triggers ... ok\ntest prune_tx_ops_count_present_indexes ... ok\ntest prune_cursor_transitions_are_monotonic ... ok\ntest retain_count_uses_capacity_and_retention_policy ... ok\ntest ratio_rounds_down_and_clamp_raises_minimum ... ok\ntest watermarks_preserve_order_under_headroom ... ok\n\ntest result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 45 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 fil
