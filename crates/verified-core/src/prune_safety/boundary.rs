//! どこで: pruning境界 / 何を: pruned_before_block更新の安全条件 / なぜ: boundary単調性とretained range保持を検証するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

pub fn prune_boundary_safe(
    previous: Option<u64>,
    next: Option<u64>,
    head: u64,
    retain: u64,
) -> bool {
    let Some(next_boundary) = next else {
        return true;
    };
    if retain == 0 || head <= retain || next_boundary > head - retain {
        return false;
    }
    match previous {
        Some(previous_boundary) => previous_boundary <= next_boundary,
        None => true,
    }
}
