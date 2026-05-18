//! どこで: pruning境界 / 何を: pruned_before_block更新の安全条件 / なぜ: boundary単調性とretained range保持を検証するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

// specgen:contract prune_boundary_safe-77bde266 b333ea9039f7ea43a15bce9b691bec941e5345525fb4689756d2b030b77dae30
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (next_present == false || (retain > 0 && head > retain && next_boundary <= head - retain && (previous_present == false || previous <= next_boundary))),
))]
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
