# Ethereum REVERT fixtures

Source: https://github.com/ethereum/tests

Commit: `c67e485ff8b5be9abc8ad15345ec21aa22e290d9`

Archive: https://raw.githubusercontent.com/ethereum/tests/c67e485ff8b5be9abc8ad15345ec21aa22e290d9/fixtures_general_state_tests.tgz

JSON files are byte-for-byte copies from `GeneralStateTests/stRevertTest/` in
the archive. `LICENSE` comes from the same commit. `SHA256SUMS` records the copies.
The `secretKey` fields are public Ethereum test vectors, not deployment credentials.

| File | Prague vectors |
| --- | ---: |
| RevertInCallCode.json | 1 |
| RevertInDelegateCall.json | 1 |
| RevertSubCallStorageOOG.json | 4 |
| RevertOpcodeMultipleSubCalls.json | 32 |

Run: `cargo test -p ic-evm-core --test revm_state_fixtures`.
The limited runner supports these legacy CALL fixtures and compares independently
supplied post-state and logs hashes. It never generates expected hashes from revm.
Cancun entries are retained unchanged; only Prague entries are executed.
