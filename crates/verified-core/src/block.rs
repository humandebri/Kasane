//! どこで: block生成境界 / 何を: 高さ・時刻・gas・命令予算判定 / なぜ: 実行前後の停止条件を純粋化するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[allow(dead_code)]
fn main() {}

#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (max_txs > 0),
))]
pub fn valid_block_limit(max_txs: usize) -> bool {
    max_txs > 0
}

#[cfg_attr(verus_keep_ghost, verus_spec(number => ensures
    head_number < u64::MAX ==> number == head_number + 1,
    head_number == u64::MAX ==> number == u64::MAX,
    number >= head_number,
))]
pub fn next_block_number(head_number: u64) -> u64 {
    head_number.saturating_add(1)
}

#[cfg_attr(verus_keep_ghost, verus_spec(timestamp => ensures
    timestamp >= now_timestamp,
    timestamp >= head_timestamp,
    head_timestamp < u64::MAX && head_timestamp + 1 >= now_timestamp
        ==> timestamp == head_timestamp + 1,
    head_timestamp < u64::MAX && head_timestamp + 1 < now_timestamp
        ==> timestamp == now_timestamp,
    head_timestamp == u64::MAX ==> timestamp == u64::MAX,
))]
pub fn next_block_timestamp(head_timestamp: u64, now_timestamp: u64) -> u64 {
    head_timestamp.saturating_add(1).max(now_timestamp)
}

#[cfg_attr(verus_keep_ghost, verus_spec(consumed => ensures
    instruction_current >= instruction_start ==> consumed == instruction_current - instruction_start,
    instruction_current < instruction_start ==> consumed == 0,
    consumed <= instruction_current,
))]
pub fn instruction_consumed(instruction_start: u64, instruction_current: u64) -> u64 {
    instruction_current.saturating_sub(instruction_start)
}

#[cfg_attr(verus_keep_ghost, verus_spec(exhausted => ensures
    exhausted == (
        instruction_soft_limit > 0
        && (if instruction_current >= instruction_start {
            instruction_current - instruction_start
        } else {
            0
        }) >= instruction_soft_limit
    ),
))]
pub fn instruction_limit_exhausted(
    instruction_soft_limit: u64,
    instruction_start: u64,
    instruction_current: u64,
) -> bool {
    instruction_soft_limit > 0
        && instruction_consumed(instruction_start, instruction_current) >= instruction_soft_limit
}

// specgen:contract should_stop_execution-207b8917 3f765260952e27a8ec3a0314c78c742a43785b862bed08f1b0093bf859e36e89
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == ((block_gas_limit > 0 && block_gas_used >= block_gas_limit) || (instruction_soft_limit > 0 && (if instruction_current >= instruction_start { instruction_current - instruction_start } else { 0 }) >= instruction_soft_limit)),
))]
pub fn should_stop_execution(
    block_gas_used: u64,
    block_gas_limit: u64,
    instruction_soft_limit: u64,
    instruction_start: u64,
    instruction_current: u64,
) -> bool {
    (block_gas_limit > 0 && block_gas_used >= block_gas_limit)
        || instruction_limit_exhausted(
            instruction_soft_limit,
            instruction_start,
            instruction_current,
        )
}

#[cfg_attr(verus_keep_ghost, verus_spec(remaining => ensures
    instruction_soft_limit == 0 ==> remaining == Option::<u64>::None,
    instruction_soft_limit > 0 ==> matches!(remaining, Some(_)),
))]
pub fn remaining_instruction_budget(
    instruction_soft_limit: u64,
    instruction_start: u64,
    instruction_current: u64,
) -> Option<u64> {
    if instruction_soft_limit == 0 {
        None
    } else {
        Some(
            instruction_soft_limit
                .saturating_sub(instruction_consumed(instruction_start, instruction_current)),
        )
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(fits => ensures
    fits == (block_gas_limit == 0 || tx_gas_limit <= block_gas_limit.saturating_sub(block_gas_used)),
))]
pub fn tx_fits_block_gas(block_gas_used: u64, block_gas_limit: u64, tx_gas_limit: u64) -> bool {
    block_gas_limit == 0 || tx_gas_limit <= block_gas_limit.saturating_sub(block_gas_used)
}

#[cfg_attr(verus_keep_ghost, verus_spec(total => ensures
    block_gas_used as int + tx_gas_used as int <= u64::MAX as int
        ==> total == block_gas_used + tx_gas_used,
    block_gas_used as int + tx_gas_used as int > u64::MAX as int
        ==> total == u64::MAX,
    total >= block_gas_used,
))]
pub fn add_block_gas_used(block_gas_used: u64, tx_gas_used: u64) -> u64 {
    block_gas_used.saturating_add(tx_gas_used)
}

#[cfg(test)]
mod tests {
    use super::{
        add_block_gas_used, next_block_number, next_block_timestamp, remaining_instruction_budget,
        should_stop_execution, tx_fits_block_gas, valid_block_limit,
    };

    #[test]
    fn block_height_and_timestamp_are_monotonic() {
        assert!(valid_block_limit(1));
        assert!(!valid_block_limit(0));
        assert_eq!(next_block_number(41), 42);
        assert_eq!(next_block_number(u64::MAX), u64::MAX);
        assert_eq!(next_block_timestamp(10, 8), 11);
        assert_eq!(next_block_timestamp(10, 30), 30);
    }

    #[test]
    fn execution_stops_on_gas_or_instruction_budget() {
        assert!(should_stop_execution(10, 10, 0, 0, 0));
        assert!(!should_stop_execution(9, 10, 0, 0, 100));
        assert!(should_stop_execution(0, 0, 50, 10, 60));
        assert!(!should_stop_execution(0, 0, 0, 10, 60));
    }

    #[test]
    fn remaining_budget_and_block_gas_are_saturating() {
        assert_eq!(remaining_instruction_budget(0, 10, 60), None);
        assert_eq!(remaining_instruction_budget(50, 10, 40), Some(20));
        assert_eq!(remaining_instruction_budget(50, 100, 40), Some(50));
        assert!(tx_fits_block_gas(90, 100, 10));
        assert!(!tx_fits_block_gas(91, 100, 10));
        assert!(tx_fits_block_gas(91, 0, 10));
        assert_eq!(add_block_gas_used(90, 10), 100);
        assert_eq!(add_block_gas_used(u64::MAX, 10), u64::MAX);
    }
}
