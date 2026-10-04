# Block production performance investigation (2026-10-04)

The failed CI measurement did not execute an EVM transaction. The historical
`produce_block_path` fixture submitted a transaction with a priority fee below
the configured floor, ignored `InvalidFee`, and measured `QueueEmpty` from
`produce_block(1)`. Its reported +8.49% instruction count was an empty-queue
overhead regression, not a measurement of EVM execution throughput.

The fixture now asserts both errors. Its name and the existing instruction
baseline (4,274) remain unchanged. A separate `produce_block_funded_path` funds
the sender, uses the configured fee floors, and checks that the transaction is
included with a successful receipt. Funding and receipt assertions occur
outside the measured interval.

## Comparison

All three revisions were measured using the same final benchmark fixtures,
Rust 1.99.0 (`b940084d7`), the repository's release profile (`opt-level=z`, LTO,
one codegen unit), canbench 0.4.1, and PocketIC 10.0.0 on macOS arm64.
The baseline revision is `32160d7e`; the implementation before this optimization
is `7f32843b`. These are Wasm instructions, not elapsed-time measurements.

| Path | Main | Implementation before optimization | Optimized implementation |
| --- | ---: | ---: | ---: |
| Empty queue (`produce_block_path`) | 4,148 | 4,644 | 4,318 |
| Successful funded transaction | 20,439,285 | 20,442,124 | 20,441,798 |

The optimization removes 326 instructions from both paths. It keeps the owned
query session and reply buffers out of the ordinary block path and reuses the
session already captured by the synchronous block dispatcher, rather than
reading/cloning it again. The public entry point retains limit validation;
waiting, replay, snapshot, and cleanup behavior retain their existing checks.

The empty path still adds 170 instructions against freshly measured main
(+4.10%). The remaining query-state check is required to prevent ordinary blocks
from overtaking a suspended transaction. The successful funded fixture adds
2,513 instructions (+0.0123%). This narrow measurement supports accepting the
remaining ordinary-path overhead; it does not characterize all contracts or the
cost of external-query transactions.

The existing CI threshold stays at +2%, and the existing baselines are not
raised. The optimized empty fixture is +1.03% against the stored 4,274 baseline.
The new successful-path baseline is the measured **main** value, 20,439,285,
rather than the implementation's value. All ten local measurements pass the
existing threshold checker.

## Reproduction and validation

Apply the same `canbench_benches.rs` fixture to each isolated revision and run:

```sh
rustc +stable --version # This comparison used 1.99.0.
RUSTUP_TOOLCHAIN=stable .canbench-tools/bin/canbench --persist
scripts/check_canbench_thresholds.sh baseline.yml canbench_results.yml
```

For these local measurements, the official arm64 PocketIC 10.0.0 binary was
downloaded from the DFINITY release and supplied with `--runtime-path` and
`--no-runtime-integrity-check`. canbench 0.4.1 otherwise selects the Intel macOS
runtime, which cannot execute on this host. The same arm64 binary was used for
all three revisions. CI's runtime selection and integrity check are unchanged;
its Linux results must be checked separately.

Validation after optimization: workspace check, query transaction regression
tests (18, including invalid-limit/session preservation and missing-payload
cleanup), core/gateway clippy with all targets/features and warnings denied,
and PocketIC query transaction E2E (3 tests). The Lean/Verus model sources and
contracts are unchanged by this optimization.
