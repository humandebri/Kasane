//! どこで: EVM canister中核遷移 / 何を: submit・include・commitの安全条件 / なぜ: 外部境界を除いた不変条件を固定するため

use crate::block::tx_fits_block_gas;
use crate::block_persist::{classify_persist_batch, PersistBatchDecision};
use crate::nonce::NonceDecision;
use crate::tx_index::{included_position_matches, IncludedTxPosition};

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
use crate::tx_index::TX_LOC_KIND_INCLUDED;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubmitTransitionFacts {
    pub decision: NonceDecision,
    pub pending_slot_points_to_new: bool,
    pub new_current_written: bool,
    pub queued_loc_written: bool,
    pub replacement_old_removed: bool,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IncludedTxFacts {
    pub has_tx_index: bool,
    pub has_receipt: bool,
    pub receipt_tx_id_matches: bool,
    pub index_key_matches_tx_id: bool,
    pub loc_matches_position: bool,
    pub receipt_matches_position: bool,
    pub index_matches_position: bool,
    pub block_number: u64,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockCommitFacts {
    pub previous_head: u64,
    pub committed_head: u64,
    pub included_count: usize,
    pub staged_count: usize,
    pub safe_included_count: usize,
    pub block_gas_used: u64,
    pub block_gas_limit: u64,
}

#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == match facts.decision {
        NonceDecision::Accept =>
            facts.pending_slot_points_to_new
                && facts.new_current_written
                && facts.queued_loc_written,
        NonceDecision::Replace =>
            facts.pending_slot_points_to_new
                && facts.new_current_written
                && facts.queued_loc_written
                && facts.replacement_old_removed,
        NonceDecision::TooLow | NonceDecision::Gap | NonceDecision::Conflict => false,
    },
))]
pub fn submit_transition_safe(facts: SubmitTransitionFacts) -> bool {
    match facts.decision {
        NonceDecision::Accept => {
            facts.pending_slot_points_to_new
                && facts.new_current_written
                && facts.queued_loc_written
        }
        NonceDecision::Replace => {
            facts.pending_slot_points_to_new
                && facts.new_current_written
                && facts.queued_loc_written
                && facts.replacement_old_removed
        }
        NonceDecision::TooLow | NonceDecision::Gap | NonceDecision::Conflict => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        facts.has_tx_index
        && facts.has_receipt
        && facts.receipt_tx_id_matches
        && facts.index_key_matches_tx_id
        && facts.loc_matches_position
        && facts.receipt_matches_position
        && facts.index_matches_position
        && facts.block_number > 0
    ),
))]
pub fn included_tx_safe(facts: IncludedTxFacts) -> bool {
    facts.has_tx_index
        && facts.has_receipt
        && facts.block_number > 0
        && facts.receipt_tx_id_matches
        && facts.index_key_matches_tx_id
        && facts.loc_matches_position
        && facts.receipt_matches_position
        && facts.index_matches_position
}

#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (committed_head >= previous_head),
))]
pub fn head_transition_safe(previous_head: u64, committed_head: u64) -> bool {
    committed_head >= previous_head
}

#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (block_gas_limit == 0 || block_gas_used <= block_gas_limit),
    safe ==> (block_gas_limit == 0 || block_gas_used <= block_gas_limit),
))]
pub fn block_gas_safe(block_gas_used: u64, block_gas_limit: u64) -> bool {
    tx_fits_block_gas(0, block_gas_limit, block_gas_used)
}

#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        included_count != 0
        && included_count == staged_count
        && safe_included_count == included_count
    ),
))]
pub fn included_batch_safe(
    included_count: usize,
    staged_count: usize,
    safe_included_count: usize,
) -> bool {
    let decision = classify_persist_batch(included_count, staged_count);
    match decision {
        PersistBatchDecision::Commit => safe_included_count == included_count,
        PersistBatchDecision::CountMismatch | PersistBatchDecision::Empty => false,
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        facts.committed_head >= facts.previous_head
        && (facts.block_gas_limit == 0 || facts.block_gas_used <= facts.block_gas_limit)
        &&
        facts.included_count != 0
        && facts.included_count == facts.staged_count
        && facts.safe_included_count == facts.included_count
    ),
))]
pub fn block_commit_safe(facts: BlockCommitFacts) -> bool {
    facts.committed_head >= facts.previous_head
        && (facts.block_gas_limit == 0 || facts.block_gas_used <= facts.block_gas_limit)
        && facts.included_count != 0
        && facts.included_count == facts.staged_count
        && facts.safe_included_count == facts.included_count
}

#[cfg_attr(verus_keep_ghost, verus_spec(matches => ensures
    matches == (
        loc_kind == TX_LOC_KIND_INCLUDED
        && receipt_block_number == position.block_number
        && receipt_tx_index == position.tx_index
    ),
))]
pub fn receipt_position_matches(
    position: IncludedTxPosition,
    loc_kind: u8,
    receipt_block_number: u64,
    receipt_tx_index: u32,
) -> bool {
    included_position_matches(position, loc_kind, receipt_block_number, receipt_tx_index)
}

#[cfg(test)]
mod tests {
    use super::{
        block_commit_safe, block_gas_safe, head_transition_safe, included_batch_safe,
        included_tx_safe, submit_transition_safe, BlockCommitFacts, IncludedTxFacts,
        SubmitTransitionFacts,
    };
    use crate::nonce::NonceDecision;

    #[test]
    fn submit_transition_requires_nonce_acceptance_and_adapter_writes() {
        assert!(submit_transition_safe(SubmitTransitionFacts {
            decision: NonceDecision::Accept,
            pending_slot_points_to_new: true,
            new_current_written: true,
            queued_loc_written: true,
            replacement_old_removed: false,
        }));
        assert!(submit_transition_safe(SubmitTransitionFacts {
            decision: NonceDecision::Replace,
            pending_slot_points_to_new: true,
            new_current_written: true,
            queued_loc_written: true,
            replacement_old_removed: true,
        }));
        assert!(!submit_transition_safe(SubmitTransitionFacts {
            decision: NonceDecision::Replace,
            pending_slot_points_to_new: true,
            new_current_written: true,
            queued_loc_written: true,
            replacement_old_removed: false,
        }));
        assert!(!submit_transition_safe(SubmitTransitionFacts {
            decision: NonceDecision::Gap,
            pending_slot_points_to_new: true,
            new_current_written: true,
            queued_loc_written: true,
            replacement_old_removed: true,
        }));
    }

    #[test]
    fn included_tx_requires_receipt_index_and_position_evidence() {
        assert!(included_tx_safe(IncludedTxFacts {
            has_tx_index: true,
            has_receipt: true,
            receipt_tx_id_matches: true,
            index_key_matches_tx_id: true,
            loc_matches_position: true,
            receipt_matches_position: true,
            index_matches_position: true,
            block_number: 1,
        }));
        assert!(!included_tx_safe(IncludedTxFacts {
            has_tx_index: true,
            has_receipt: false,
            receipt_tx_id_matches: true,
            index_key_matches_tx_id: true,
            loc_matches_position: true,
            receipt_matches_position: true,
            index_matches_position: true,
            block_number: 1,
        }));
        assert!(!included_tx_safe(IncludedTxFacts {
            has_tx_index: true,
            has_receipt: true,
            receipt_tx_id_matches: false,
            index_key_matches_tx_id: true,
            loc_matches_position: true,
            receipt_matches_position: true,
            index_matches_position: true,
            block_number: 1,
        }));
    }

    #[test]
    fn block_commit_requires_head_gas_and_batch_evidence() {
        assert!(head_transition_safe(1, 2));
        assert!(!head_transition_safe(2, 1));
        assert!(block_gas_safe(10, 10));
        assert!(!block_gas_safe(11, 10));
        assert!(included_batch_safe(2, 2, 2));
        assert!(!included_batch_safe(2, 2, 1));
        assert!(block_commit_safe(BlockCommitFacts {
            previous_head: 1,
            committed_head: 2,
            included_count: 2,
            staged_count: 2,
            safe_included_count: 2,
            block_gas_used: 10,
            block_gas_limit: 10,
        }));
    }
}
