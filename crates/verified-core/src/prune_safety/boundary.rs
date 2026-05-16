//! どこで: pruning境界 / 何を: pruned_before_block更新の安全条件 / なぜ: boundary単調性とretained range保持を検証するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

pub fn prune_boundary_safe(previous_present: bool, previous: u64, next_present: bool, next_boundary: u64, head: u64, retain: u64) -> bool
{
    if !next_present {
        return true;
    }
    if retain == 0 || head <= retain || next_boundary > head - retain {
        return false;
    }
    if previous_present {
        previous <= next_boundary
    } else {
        true
    }
}
