//! どこで: prune policy計算 / 何を: watermarkとmax_ops clamp / なぜ: 運用設定を決定的にするため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
const BPS_DENOMINATOR: u128 = 10_000;
#[cfg_attr(verus_keep_ghost, verus_verify)]
const SECONDS_PER_DAY: u64 = 86_400;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetainCountInput {
    pub head_block: u64,
    pub target_bytes: u64,
    pub estimated_kept_bytes: u64,
    pub high_water_bytes: u64,
    pub hard_emergency_bytes: u64,
    pub retain_blocks: u64,
    pub retain_days: u64,
    pub cutoff_block: Option<u64>,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PruneCursor {
    pub next_prune_block: u64,
    pub pruned_before_block: Option<u64>,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PruneTxPresence {
    pub pending_fee_index: bool,
    pub principal_pending_count: bool,
    pub eth_tx_hash_index: bool,
    pub tx_store: bool,
    pub receipt: bool,
    pub tx_index: bool,
    pub internal_traces: bool,
    pub tx_loc: bool,
    pub seen_tx: bool,
}

#[cfg_attr(verus_keep_ghost, verus_spec(bytes => ensures
    bytes as int == if ((target as int) * (ratio_bps as int)) / 10_000int
        > u64::MAX as int {
        u64::MAX as int
    } else {
        ((target as int) * (ratio_bps as int)) / 10_000int
    },
))]
pub fn ratio_bytes(target: u64, ratio_bps: u32) -> u64 {
    let ratio = u128::from(ratio_bps);
    #[cfg(verus_keep_ghost)]
    proof! {
        assert((target as int) * (ratio_bps as int) <= u128::MAX as int)
            by(nonlinear_arith);
    }
    let numerator = u128::from(target) * ratio;
    let bytes = numerator / BPS_DENOMINATOR;
    let result = if bytes > u128::from(u64::MAX) {
        u64::MAX
    } else {
        bytes as u64
    };
    result
}

#[cfg_attr(verus_keep_ghost, verus_spec(bytes => ensures
    headroom_bps == 0 ==> bytes == target,
))]
pub fn high_water(target: u64, headroom_bps: u32) -> u64 {
    if headroom_bps == 0 {
        return target;
    }
    let ratio = 10_000u32.saturating_sub(headroom_bps / 2);
    ratio_bytes(target, ratio)
}

#[cfg_attr(verus_keep_ghost, verus_spec(bytes => ensures
    headroom_bps == 0 ==> bytes == target,
))]
pub fn low_water(target: u64, headroom_bps: u32) -> u64 {
    if headroom_bps == 0 {
        return target;
    }
    let ratio = 10_000u32.saturating_sub(headroom_bps);
    ratio_bytes(target, ratio)
}

#[cfg_attr(verus_keep_ghost, verus_spec(clamped => ensures
    clamped >= requested,
    clamped >= minimum,
    clamped == requested || clamped == minimum,
))]
pub fn clamp_max_ops(requested: u32, minimum: u32) -> u32 {
    requested.max(minimum)
}

#[cfg_attr(verus_keep_ghost, verus_spec(needed => ensures
    needed == (
        (
            retain_days > 0
            && matches!(oldest_timestamp, Some(_))
            && oldest_timestamp.unwrap() < if now >= if retain_days > u64::MAX / 86_400 {
                u64::MAX
            } else {
                (retain_days * 86_400) as u64
            } {
                now - if retain_days > u64::MAX / 86_400 {
                    u64::MAX
                } else {
                    (retain_days * 86_400) as u64
                }
            } else {
                0
            }
        )
        || (target_bytes > 0 && estimated_kept_bytes > high_water_bytes)
    ),
))]
pub fn need_prune(
    retain_days: u64,
    oldest_timestamp: Option<u64>,
    now: u64,
    target_bytes: u64,
    estimated_kept_bytes: u64,
    high_water_bytes: u64,
) -> bool {
    let time_trigger = if retain_days > 0 {
        match oldest_timestamp {
            Some(oldest_ts) => {
                let retain_secs = if retain_days > u64::MAX / SECONDS_PER_DAY {
                    u64::MAX
                } else {
                    retain_days * SECONDS_PER_DAY
                };
                oldest_ts < now.saturating_sub(retain_secs)
            }
            None => false,
        }
    } else {
        false
    };
    let cap_trigger = target_bytes > 0 && estimated_kept_bytes > high_water_bytes;
    time_trigger || cap_trigger
}

#[cfg_attr(verus_keep_ghost, verus_spec(retain => ensures
    retain >= 1,
    input.target_bytes > 0 && input.estimated_kept_bytes > input.hard_emergency_bytes
        ==> retain == 1,
    input.target_bytes > 0 && input.estimated_kept_bytes > input.high_water_bytes
        ==> retain == 1,
    !(input.target_bytes > 0 && input.estimated_kept_bytes > input.hard_emergency_bytes)
        && !(input.target_bytes > 0 && input.estimated_kept_bytes > input.high_water_bytes)
        && input.retain_blocks == 0
        && (input.retain_days == 0 || matches!(input.cutoff_block, None))
        && input.head_block < u64::MAX
        ==> retain == input.head_block + 1,
    !(input.target_bytes > 0 && input.estimated_kept_bytes > input.hard_emergency_bytes)
        && !(input.target_bytes > 0 && input.estimated_kept_bytes > input.high_water_bytes)
        && input.retain_blocks > 0
        && input.retain_blocks <= input.head_block + 1
        && (input.retain_days == 0 || matches!(input.cutoff_block, None))
        ==> retain == input.retain_blocks,
))]
pub fn retain_count(input: RetainCountInput) -> u64 {
    let emergency =
        input.target_bytes > 0 && input.estimated_kept_bytes > input.hard_emergency_bytes;
    let cap_trigger = input.target_bytes > 0 && input.estimated_kept_bytes > input.high_water_bytes;
    if emergency || cap_trigger {
        return 1;
    }

    let mut retain_min_block = 0u64;
    if input.retain_blocks > 0 {
        retain_min_block = input
            .head_block
            .saturating_sub(input.retain_blocks.saturating_sub(1));
    }
    if input.retain_days > 0 {
        if let Some(cutoff_block) = input.cutoff_block {
            retain_min_block = retain_min_block.max(cutoff_block);
        }
    }

    input
        .head_block
        .saturating_sub(retain_min_block)
        .saturating_add(1)
        .max(1)
}

#[cfg_attr(verus_keep_ghost, verus_spec(block => ensures
    retain == 0 ==> block == Option::<u64>::None,
    head_block <= retain ==> block == Option::<u64>::None,
    retain > 0 && head_block > retain ==> block == Option::<u64>::Some((head_block - retain) as u64),
))]
pub fn prune_before_block(head_block: u64, retain: u64) -> Option<u64> {
    if retain == 0 || head_block <= retain {
        None
    } else {
        Some(head_block.saturating_sub(retain))
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(next => ensures
    matches!(cursor.pruned_before_block, Some(_))
        && cursor.next_prune_block <= cursor.pruned_before_block.unwrap()
        && cursor.pruned_before_block.unwrap() < u64::MAX
        ==> next == cursor.pruned_before_block.unwrap() + 1,
    matches!(cursor.pruned_before_block, Some(_))
        && cursor.next_prune_block <= cursor.pruned_before_block.unwrap()
        && cursor.pruned_before_block.unwrap() == u64::MAX
        ==> next == u64::MAX,
    !(matches!(cursor.pruned_before_block, Some(_))
        && cursor.next_prune_block <= cursor.pruned_before_block.unwrap())
        ==> next == cursor.next_prune_block,
    next >= cursor.next_prune_block,
))]
pub fn normalize_next_prune_block(cursor: PruneCursor) -> u64 {
    match cursor.pruned_before_block {
        Some(pruned) if cursor.next_prune_block <= pruned => pruned.saturating_add(1),
        _ => cursor.next_prune_block,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(cursor => ensures
    block_number < u64::MAX ==> cursor.next_prune_block == block_number + 1,
    block_number == u64::MAX ==> cursor.next_prune_block == u64::MAX,
    cursor.pruned_before_block == Option::<u64>::Some(block_number),
))]
pub fn advance_after_pruned_block(block_number: u64) -> PruneCursor {
    PruneCursor {
        next_prune_block: block_number.saturating_add(1),
        pruned_before_block: Some(block_number),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(recovered => ensures
    matches!(current, None) ==> recovered == Option::<u64>::Some(journal_block),
    matches!(current, Some(_)) && current.unwrap() >= journal_block
        ==> recovered == current,
    matches!(current, Some(_)) && current.unwrap() < journal_block
        ==> recovered == Option::<u64>::Some(journal_block),
))]
pub fn recover_pruned_before(current: Option<u64>, journal_block: u64) -> Option<u64> {
    Some(match current {
        Some(pruned) => pruned.max(journal_block),
        None => journal_block,
    })
}

#[cfg_attr(verus_keep_ghost, verus_spec(remaining => ensures
    next_prune_block > prune_before ==> remaining == 0,
    next_prune_block <= prune_before && prune_before < u64::MAX
        ==> remaining == prune_before - next_prune_block + 1,
    next_prune_block == 0 && prune_before == u64::MAX ==> remaining == u64::MAX,
    next_prune_block > 0 && next_prune_block <= prune_before && prune_before == u64::MAX
        ==> remaining == u64::MAX - next_prune_block + 1,
))]
pub fn remaining_blocks(next_prune_block: u64, prune_before: u64) -> u64 {
    if next_prune_block > prune_before {
        0
    } else {
        prune_before
            .saturating_sub(next_prune_block)
            .saturating_add(1)
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(needed => ensures
    needed <= 9,
    !input.pending_fee_index
        && !input.principal_pending_count
        && !input.eth_tx_hash_index
        && !input.tx_store
        && !input.receipt
        && !input.tx_index
        && !input.internal_traces
        && !input.tx_loc
        && !input.seen_tx
        ==> needed == 0,
    input.pending_fee_index
        && input.principal_pending_count
        && input.eth_tx_hash_index
        && input.tx_store
        && input.receipt
        && input.tx_index
        && input.internal_traces
        && input.tx_loc
        && input.seen_tx
        ==> needed == 9,
))]
pub fn prune_ops_needed_for_tx(input: PruneTxPresence) -> u64 {
    let mut needed = 0u64;
    if input.pending_fee_index {
        needed += 1;
    }
    if input.principal_pending_count {
        needed += 1;
    }
    if input.eth_tx_hash_index {
        needed += 1;
    }
    if input.tx_store {
        needed += 1;
    }
    if input.receipt {
        needed += 1;
    }
    if input.tx_index {
        needed += 1;
    }
    if input.internal_traces {
        needed += 1;
    }
    if input.tx_loc {
        needed += 1;
    }
    if input.seen_tx {
        needed += 1;
    }
    needed
}
