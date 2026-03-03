# ICP-EVM ic-wasm Workflow

Japanese version: [./ic-wasm-workflow.ja.md](./ic-wasm-workflow.ja.md)

## Where/What/Why
Operational workflow for wasm post-processing and profiling in ICP-EVM deployments.

## Included Topics
- standard post-build pipeline
- integration points into existing deploy paths
- `ci-local` mode operations
- WASI stubbing (`--stub-wasi`)
- profiling flow
- BLOCK_GAS_LIMIT precision procedure
- validation commands
- worktree initialization (`vendor/revm`)
- observability and benchmark additions (Prometheus, canbench)

## Usage
Follow this as the canonical process for build artifact handling and performance measurement.
For concrete command-level details, see the Japanese version.
