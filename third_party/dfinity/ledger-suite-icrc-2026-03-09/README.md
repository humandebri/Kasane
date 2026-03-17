# Vendored official ICRC-1 ledger wasm

- Source release: `ledger-suite-icrc-2026-03-09`
- Source URL: `https://github.com/dfinity/ic/releases/download/ledger-suite-icrc-2026-03-09/ic-icrc1-ledger.wasm.gz`
- File: `ic-icrc1-ledger.wasm`
- SHA-256: `a273d741019b4324b22e2e64cfb0239de148ae9693e8801102a04118dfd383c0`

This wasm is committed so local CI and PocketIC E2E do not depend on a network download.

Only the wasm is vendored in this repository. The matching `ledger.did` is cached per release under `${LEDGER_CACHE_DIR}/<release>/ledger.did` by the local smoke script.
