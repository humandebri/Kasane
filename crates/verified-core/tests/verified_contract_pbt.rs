//! どこで: verified-core PBT / 何を: Verus対象述語の外形仕様 / なぜ: 証明済み仕様自体の誤りを乱択で検出するため

use proptest::prelude::*;
use verified_core::block::should_stop_execution;
use verified_core::core_safety::{
    submit_transition_safe_raw, SUBMIT_DECISION_ACCEPT, SUBMIT_DECISION_REPLACE,
};
use verified_core::core_safety_block::block_commit_safe_raw;
use verified_core::core_safety_included::included_tx_safe_raw;
use verified_core::no_reorg::no_reorg_append_only_raw;
use verified_core::nonce::{classify_nonce, NonceDecision};
use verified_core::prune_safety::{prune_tx_cleanup_complete, PruneTxCleanupInput};
use verified_core::receipt_index::{
    receipt_index_location_bidirectional, receipt_index_target_observation_safe,
    ReceiptIndexObservation,
};
use verified_core::stable_namespace::stable_tx_namespace_disjoint_raw;
use verified_core::staging::staged_tx_is_current_pending_raw;
use verified_core::upgrade_safety::upgrade_core_observation_preserved_raw;

fn is_one(value: u64) -> bool {
    value == 1
}

#[allow(clippy::nonminimal_bool)]
fn expected_receipt_index(observation: ReceiptIndexObservation) -> bool {
    (!observation.tx_index_present
        || (observation.receipt_present
            && observation.included_loc_present
            && observation.index_matches_loc))
        && (!observation.receipt_present
            || (observation.tx_index_present
                && observation.included_loc_present
                && observation.receipt_matches_loc))
        && (!observation.included_loc_present
            || (observation.tx_index_present
                && observation.receipt_present
                && observation.index_matches_loc
                && observation.receipt_matches_loc
                && observation.loc_points_to_block_tx))
}

fn expected_target_receipt_index(
    target_included: bool,
    observation: ReceiptIndexObservation,
) -> bool {
    expected_receipt_index(observation)
        && ((target_included
            && observation.tx_index_present
            && observation.receipt_present
            && observation.included_loc_present
            && observation.index_matches_loc
            && observation.receipt_matches_loc
            && observation.loc_points_to_block_tx)
            || (!target_included
                && !observation.tx_index_present
                && !observation.receipt_present
                && !observation.included_loc_present))
}

proptest! {
    #[test]
    fn pbt_nonce_decision_matches_total_order(
        expected_nonce in any::<u64>(),
        incoming_nonce in any::<u64>(),
        pending_effective_gas_price in proptest::option::of(any::<u64>()),
        incoming_effective_gas_price in any::<u64>(),
    ) {
        let expected = if incoming_nonce < expected_nonce {
            NonceDecision::TooLow
        } else if incoming_nonce > expected_nonce {
            NonceDecision::Gap
        } else {
            match pending_effective_gas_price {
                None => NonceDecision::Accept,
                Some(old) if incoming_effective_gas_price <= old => NonceDecision::Conflict,
                Some(_) => NonceDecision::Replace,
            }
        };

        prop_assert_eq!(
            classify_nonce(
                expected_nonce,
                incoming_nonce,
                pending_effective_gas_price,
                incoming_effective_gas_price,
            ),
            expected
        );
    }

    #[test]
    fn pbt_should_stop_execution_matches_gas_or_instruction_exhaustion(
        block_gas_used in any::<u64>(),
        block_gas_limit in any::<u64>(),
        instruction_soft_limit in any::<u64>(),
        instruction_start in any::<u64>(),
        instruction_current in any::<u64>(),
    ) {
        let consumed = instruction_current.saturating_sub(instruction_start);
        let expected = (block_gas_limit > 0 && block_gas_used >= block_gas_limit)
            || (instruction_soft_limit > 0 && consumed >= instruction_soft_limit);

        prop_assert_eq!(
            should_stop_execution(
                block_gas_used,
                block_gas_limit,
                instruction_soft_limit,
                instruction_start,
                instruction_current,
            ),
            expected
        );
    }

    #[test]
    fn pbt_block_commit_requires_next_head_matching_counts_and_gas_limit(
        previous_head in any::<u64>(),
        committed_head in any::<u64>(),
        included_count in any::<u64>(),
        staged_count in any::<u64>(),
        safe_included_count in any::<u64>(),
        block_gas_used in any::<u64>(),
        block_gas_limit in any::<u64>(),
    ) {
        let expected = previous_head.checked_add(1) == Some(committed_head)
            && (block_gas_limit == 0 || block_gas_used <= block_gas_limit)
            && included_count != 0
            && included_count == staged_count
            && safe_included_count == included_count;

        prop_assert_eq!(
            block_commit_safe_raw(
                previous_head,
                committed_head,
                included_count,
                staged_count,
                safe_included_count,
                block_gas_used,
                block_gas_limit,
            ),
            expected
        );
    }

    #[test]
    fn pbt_included_tx_safe_accepts_only_complete_canonical_observation(
        has_tx_index in 0u64..4,
        has_receipt in 0u64..4,
        receipt_tx_id_matches in 0u64..4,
        index_key_matches_tx_id in 0u64..4,
        loc_matches_position in 0u64..4,
        receipt_matches_position in 0u64..4,
        index_matches_position in 0u64..4,
        block_number in any::<u64>(),
    ) {
        let expected = is_one(has_tx_index)
            && is_one(has_receipt)
            && is_one(receipt_tx_id_matches)
            && is_one(index_key_matches_tx_id)
            && is_one(loc_matches_position)
            && is_one(receipt_matches_position)
            && is_one(index_matches_position)
            && block_number > 0;

        prop_assert_eq!(
            included_tx_safe_raw(
                has_tx_index,
                has_receipt,
                receipt_tx_id_matches,
                index_key_matches_tx_id,
                loc_matches_position,
                receipt_matches_position,
                index_matches_position,
                block_number,
            ),
            expected
        );
    }

    #[test]
    fn pbt_submit_staging_upgrade_and_reorg_require_exact_witness_flags(
        decision_code in 0u64..4,
        a in 0u64..4,
        b in 0u64..4,
        c in 0u64..4,
        d in 0u64..4,
        e in 0u64..4,
        previous_head in any::<u64>(),
        committed_head in any::<u64>(),
    ) {
        let expected_submit = (decision_code == SUBMIT_DECISION_ACCEPT && a == 1 && b == 1 && c == 1 && d == 0)
            || (decision_code == SUBMIT_DECISION_REPLACE && a == 1 && b == 1 && c == 1 && d == 1);
        prop_assert_eq!(submit_transition_safe_raw(decision_code, a, b, c, d), expected_submit);

        prop_assert_eq!(
            staged_tx_is_current_pending_raw(a, b, c, d, e),
            a == 1 && b == 1 && c == 1 && d == 1 && e == 1
        );
        prop_assert_eq!(
            upgrade_core_observation_preserved_raw(a, b, c, d, e, 1),
            a == 1 && b == 1 && c == 1 && d == 1 && e == 1
        );
        prop_assert_eq!(
            no_reorg_append_only_raw(previous_head, committed_head, a, b, c, d),
            previous_head.checked_add(1) == Some(committed_head)
                && a == 1 && b == 1 && c == 1 && d == 1
        );
    }

    #[test]
    fn pbt_stable_namespace_is_strictly_ordered(
        seen_tx in any::<u64>(),
        tx_store in any::<u64>(),
        tx_index in any::<u64>(),
        receipts in any::<u64>(),
        tx_locs in any::<u64>(),
        tx_locs_v3 in any::<u64>(),
        internal_traces in any::<u64>(),
    ) {
        prop_assert_eq!(
            stable_tx_namespace_disjoint_raw(
                seen_tx,
                tx_store,
                tx_index,
                receipts,
                tx_locs,
                tx_locs_v3,
                internal_traces,
            ),
            seen_tx < tx_store
                && tx_store < tx_index
                && tx_index < receipts
                && receipts < tx_locs
                && tx_locs < tx_locs_v3
                && tx_locs_v3 < internal_traces
        );
    }

    #[test]
    fn pbt_receipt_and_cleanup_observations_require_all_links_or_no_links(
        target_included in any::<bool>(),
        tx_index_present in any::<bool>(),
        receipt_present in any::<bool>(),
        included_loc_present in any::<bool>(),
        index_matches_loc in any::<bool>(),
        receipt_matches_loc in any::<bool>(),
        loc_points_to_block_tx in any::<bool>(),
    ) {
        let observation = ReceiptIndexObservation {
            tx_index_present,
            receipt_present,
            included_loc_present,
            index_matches_loc,
            receipt_matches_loc,
            loc_points_to_block_tx,
        };
        prop_assert_eq!(
            receipt_index_location_bidirectional(observation),
            expected_receipt_index(observation)
        );
        prop_assert_eq!(
            receipt_index_target_observation_safe(target_included, observation),
            expected_target_receipt_index(target_included, observation)
        );

        let cleanup = PruneTxCleanupInput {
            tx_store: tx_index_present,
            receipt: receipt_present,
            tx_index: included_loc_present,
            internal_traces: index_matches_loc,
            tx_loc: receipt_matches_loc,
            seen_tx: loc_points_to_block_tx,
        };
        prop_assert_eq!(
            prune_tx_cleanup_complete(cleanup),
            !tx_index_present
                && !receipt_present
                && !included_loc_present
                && !index_matches_loc
                && !receipt_matches_loc
                && !loc_points_to_block_tx
        );
    }
}
