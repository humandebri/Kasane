//! どこで: pruning境界 / 何を: blockが削除可能範囲にあるか / なぜ: retained rangeを破壊しない前提を証明するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

// specgen:contract block_is_prunable-04224fd7 4c24bfecde7f3fe6b6c7e82b04837ff70c7f83f25ea60f001134083a43bc4a36
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (retain > 0 && head > retain && block <= head - retain),
))]
pub fn block_is_prunable(head: u64, retain: u64, block: u64) -> bool
{
    if retain == 0 || head <= retain {
        return false;
    }
    block <= head - retain
}
