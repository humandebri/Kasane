# edge-case review: should_stop_execution

Covered boundaries:

- gas limit enabled and reached
- gas limit disabled with zero limit
- instruction limit enabled and reached
- instruction counter below start treated as zero consumed instructions

The target delegates consumed-instruction underflow handling to
`instruction_limit_exhausted`.
