# Ethereum Compatibility Fix Plan (PR0-PR9)

Japanese version: [./fixplan2.ja.md](./fixplan2.ja.md)

## Purpose
Roadmap/checklist for staged implementation to harden Ethereum compatibility while minimizing regressions.

## Recommended Order
- PR0: diff-based test foundation
- PR1: unify tx representation (`TxIn`)
- PR2: delegate decode to library
- PR3: standard EVM execution flow
- PR4: base fee normalization
- PR5: state root normalization (highest priority)
- PR6: standardize receipt/log types
- PR7: lock error/stop reasons
- PR8: separate signature-verification responsibilities
- PR9: isolate SIMD performance work

## Additional Sections
- PR8 boundary spec and boundary error mapping
- safety hardening and migration/guard policy
- north-star design (trap elimination, state-machine approach, safe-stop)
- staged migration scope and persistence requirements

## Usage
Use this file as a tracking checklist for implementation and review sequencing.
For normative details, constraints, and exact acceptance points, refer to the Japanese version.
