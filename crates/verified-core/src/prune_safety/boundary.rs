//! どこで: pruning境界 / 何を: pruned_before_block更新の安全条件 / なぜ: boundary単調性とretained range保持を検証するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

// specgen:contract prune_boundary_safe-d0a164b9 b66de364c9253fe19244074c3dcdab0fc3fda48a19884aee41267257d5a66df8
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (!next_present || (retain > 0 && head > retain && next <= head - retain && (!previous_present || previous <= next))),
))]
pub fn prune_boundary_safe(
    previous_present: bool,
    previous: u64,
    next_present: bool,
    next: u64,
    head: u64,
    retain: u64,
) -> bool
{
    if !next_present {
        return true;
    }
    if retain == 0 || head <= retain || next > head - retain {
        return false;
    }
    if previous_present {
        previous <= next
    } else {
        true
    }
}
