//! どこで: pending index更新 / 何を: count減算とcurrent解除判定 / なぜ: map操作前後の状態遷移を純粋化するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CountAfterDecrement {
    Remove,
    Set(u32),
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromoteDecision {
    InsertFirst,
    ReplaceMin,
    KeepCurrent,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingMinAfterAdvance {
    Set(u64),
    Remove,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RebuildPendingDecision {
    CountPrincipal,
    Skip,
}

#[cfg_attr(verus_keep_ghost, verus_spec(decision => ensures
    current <= 1 ==> decision == CountAfterDecrement::Remove,
    current > 1 ==> decision == CountAfterDecrement::Set((current - 1) as u32),
))]
pub fn decrement_count(current: u32) -> CountAfterDecrement {
    if current <= 1 {
        CountAfterDecrement::Remove
    } else {
        CountAfterDecrement::Set(current - 1)
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(clear => ensures
    clear == current_tx_matches,
))]
pub fn should_clear_current(current_tx_matches: bool) -> bool {
    current_tx_matches
}

#[cfg_attr(verus_keep_ghost, verus_spec(refresh => ensures
    refresh == (current_min_nonce == Option::<u64>::Some(advanced_nonce)),
))]
pub fn should_refresh_pending_min(current_min_nonce: Option<u64>, advanced_nonce: u64) -> bool {
    current_min_nonce == Some(advanced_nonce)
}

#[cfg_attr(verus_keep_ghost, verus_spec(next => ensures
    after_nonce < u64::MAX ==> next == after_nonce + 1,
    after_nonce == u64::MAX ==> next == u64::MAX,
    next >= after_nonce,
))]
pub fn next_pending_start_nonce(after_nonce: u64) -> u64 {
    after_nonce.saturating_add(1)
}

#[cfg_attr(verus_keep_ghost, verus_spec(decision => ensures
    matches!(current_min_nonce, None) ==> decision == PromoteDecision::InsertFirst,
    matches!(current_min_nonce, Some(_)) && incoming_nonce < current_min_nonce.unwrap()
        ==> decision == PromoteDecision::ReplaceMin,
    matches!(current_min_nonce, Some(_)) && incoming_nonce >= current_min_nonce.unwrap()
        ==> decision == PromoteDecision::KeepCurrent,
))]
pub fn classify_promote(current_min_nonce: Option<u64>, incoming_nonce: u64) -> PromoteDecision {
    match current_min_nonce {
        None => PromoteDecision::InsertFirst,
        Some(current) if incoming_nonce < current => PromoteDecision::ReplaceMin,
        Some(_) => PromoteDecision::KeepCurrent,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(decision => ensures
    matches!(next_pending_nonce, Some(_))
        ==> decision == PendingMinAfterAdvance::Set(next_pending_nonce.unwrap()),
    matches!(next_pending_nonce, None) ==> decision == PendingMinAfterAdvance::Remove,
))]
pub fn pending_min_after_advance(next_pending_nonce: Option<u64>) -> PendingMinAfterAdvance {
    match next_pending_nonce {
        Some(nonce) => PendingMinAfterAdvance::Set(nonce),
        None => PendingMinAfterAdvance::Remove,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(decision => ensures
    has_pending_meta ==> decision == RebuildPendingDecision::CountPrincipal,
    !has_pending_meta ==> decision == RebuildPendingDecision::Skip,
))]
pub fn rebuild_pending_decision(has_pending_meta: bool) -> RebuildPendingDecision {
    if has_pending_meta {
        RebuildPendingDecision::CountPrincipal
    } else {
        RebuildPendingDecision::Skip
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(next => ensures
    current < u32::MAX ==> next == current + 1,
    current == u32::MAX ==> next == u32::MAX,
    next >= current,
))]
pub fn increment_count(current: u32) -> u32 {
    current.saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::{
        classify_promote, decrement_count, increment_count, next_pending_start_nonce,
        pending_min_after_advance, rebuild_pending_decision, should_clear_current,
        should_refresh_pending_min, CountAfterDecrement, PendingMinAfterAdvance, PromoteDecision,
        RebuildPendingDecision,
    };

    #[test]
    fn decrement_count_removes_at_zero_or_one() {
        assert_eq!(decrement_count(0), CountAfterDecrement::Remove);
        assert_eq!(decrement_count(1), CountAfterDecrement::Remove);
        assert_eq!(decrement_count(2), CountAfterDecrement::Set(1));
    }

    #[test]
    fn current_and_min_decisions_are_explicit() {
        assert!(should_clear_current(true));
        assert!(!should_clear_current(false));
        assert!(should_refresh_pending_min(Some(7), 7));
        assert!(!should_refresh_pending_min(Some(8), 7));
        assert_eq!(next_pending_start_nonce(7), 8);
        assert_eq!(next_pending_start_nonce(u64::MAX), u64::MAX);
    }

    #[test]
    fn promote_decision_tracks_min_nonce() {
        assert_eq!(classify_promote(None, 10), PromoteDecision::InsertFirst);
        assert_eq!(classify_promote(Some(11), 10), PromoteDecision::ReplaceMin);
        assert_eq!(classify_promote(Some(10), 10), PromoteDecision::KeepCurrent);
        assert_eq!(classify_promote(Some(9), 10), PromoteDecision::KeepCurrent);
    }

    #[test]
    fn pending_min_after_advance_sets_or_removes() {
        assert_eq!(
            pending_min_after_advance(Some(12)),
            PendingMinAfterAdvance::Set(12)
        );
        assert_eq!(
            pending_min_after_advance(None),
            PendingMinAfterAdvance::Remove
        );
    }

    #[test]
    fn rebuild_pending_counts_only_meta_entries() {
        assert_eq!(
            rebuild_pending_decision(true),
            RebuildPendingDecision::CountPrincipal
        );
        assert_eq!(
            rebuild_pending_decision(false),
            RebuildPendingDecision::Skip
        );
        assert_eq!(increment_count(0), 1);
        assert_eq!(increment_count(u32::MAX), u32::MAX);
    }
}
