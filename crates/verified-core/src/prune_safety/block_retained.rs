//! どこで: pruning境界 / 何を: blockが保持範囲にあるか / なぜ: prune対象外の履歴を明示するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

// specgen:contract block_is_retained-9d9115e5 ae5731987844b9ac331a6c90aa5a176bc5b8d059281e0aac360d60c1cc4f5302
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (block <= head && (retain == 0 || head <= retain || block > head - retain)),
))]
pub fn block_is_retained(head: u64, retain: u64, block: u64) -> bool
{
    if block > head {
        return false;
    }
    if retain == 0 || head <= retain {
        return true;
    }
    block > head - retain
}
