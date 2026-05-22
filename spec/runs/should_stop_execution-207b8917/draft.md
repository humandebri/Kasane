# Spec draft: should_stop_execution

## Function

`should_stop_execution(block_gas_used: u64, block_gas_limit: u64, instruction_soft_limit: u64, instruction_start: u64, instruction_current: u64) -> bool`

## Inferred behavior

- If `block_gas_limit > 0` and `block_gas_used >= block_gas_limit`, execution should stop.
- If `instruction_soft_limit > 0` and consumed instructions are at least the soft limit, execution should stop.
- If both limits are disabled or not reached, execution should continue.
- `instruction_current < instruction_start` is treated as zero consumed instructions through `instruction_limit_exhausted`.

## Intended behavior

Block production uses this predicate as the pure stop rule for gas and instruction budget boundaries.

## Preconditions

None.

## Postconditions

- `stop == ((block_gas_limit > 0 && block_gas_used >= block_gas_limit) || (instruction_soft_limit > 0 && consumed >= instruction_soft_limit))`
- `consumed == instruction_current - instruction_start` when `instruction_current >= instruction_start`
- `consumed == 0` when `instruction_current < instruction_start`
