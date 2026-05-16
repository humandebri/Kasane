//! どこで: pruning境界 / 何を: blockが削除可能範囲にあるか / なぜ: retained rangeを破壊しない前提を証明するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

pub fn block_is_prunable(head: u64, retain: u64, block: u64) -> bool {
    if retain == 0 || head <= retain {
        return false;
    }
    block <= head - retain
}
