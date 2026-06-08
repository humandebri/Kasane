//! どこで: verified-core prune PBT / 何を: prune系Verus対象述語 / なぜ: 境界・応答・進捗仕様の取り違えを乱択で検出するため

use proptest::prelude::*;
use verified_core::prune_safety::{
    block_is_prunable, block_is_retained, prune_boundary_safe, prune_partial_progress_safe_raw,
    prune_query_observation_safe_raw,
};

fn is_bit(value: u64) -> bool {
    value <= 1
}

#[derive(Clone, Copy)]
struct ProgressInput {
    previous_present: u64,
    previous_boundary: u64,
    next_present: u64,
    next_boundary: u64,
    next_cursor: u64,
    max_ops: u64,
    ops_used: u64,
    next_ops_needed: u64,
    did_work: u64,
    stopped_for_budget: u64,
}

fn expected_prune_progress(input: ProgressInput) -> bool {
    is_bit(input.previous_present)
        && is_bit(input.next_present)
        && is_bit(input.did_work)
        && is_bit(input.stopped_for_budget)
        && input.ops_used <= input.max_ops
        && (input.did_work == 0 || input.next_present == 1)
        && (input.previous_present == 0 || input.next_present == 1)
        && (input.did_work == 0
            || input.previous_present == 0
            || input.previous_boundary < input.next_boundary)
        && (input.previous_present == 0
            || input.next_present == 0
            || input.previous_boundary == input.next_boundary
            || input.did_work == 1)
        && (input.previous_present == 0
            || input.next_present == 0
            || input.previous_boundary <= input.next_boundary)
        && (input.next_present == 0 || input.next_boundary < input.next_cursor)
        && (input.stopped_for_budget == 0 || input.next_present == 1)
        && (input.stopped_for_budget == 0
            || input.max_ops < input.next_ops_needed
            || (input.next_ops_needed <= input.max_ops
                && input.max_ops - input.next_ops_needed < input.ops_used))
}

proptest! {
    #[test]
    fn pbt_prunable_and_retained_are_exclusive_and_cover_existing_blocks(
        head in any::<u64>(),
        retain in any::<u64>(),
        block in any::<u64>(),
    ) {
        let prunable = block_is_prunable(head, retain, block);
        let retained = block_is_retained(head, retain, block);

        prop_assert!(!(prunable && retained));
        if block <= head {
            prop_assert_ne!(prunable, retained);
        } else {
            prop_assert!(!prunable);
            prop_assert!(!retained);
        }
        if prunable {
            prop_assert!(retain > 0);
            prop_assert!(head > retain);
            prop_assert!(block <= head - retain);
        }
        if retained {
            prop_assert!(block <= head);
        }
    }

    #[test]
    fn pbt_prune_boundary_is_monotonic_and_within_prunable_range(
        previous_present in any::<bool>(),
        previous in any::<u64>(),
        next_present in any::<bool>(),
        next_boundary in any::<u64>(),
        head in any::<u64>(),
        retain in any::<u64>(),
    ) {
        let expected = !next_present
            || (retain > 0
                && head > retain
                && next_boundary <= head - retain
                && (!previous_present || previous <= next_boundary));
        let result = prune_boundary_safe(
            previous_present,
            previous,
            next_present,
            next_boundary,
            head,
            retain,
        );

        prop_assert_eq!(result, expected);
        if result && next_present {
            prop_assert!(block_is_prunable(head, retain, next_boundary));
            if previous_present {
                prop_assert!(previous <= next_boundary);
            }
        }
    }

    #[test]
    fn pbt_prune_query_observation_uses_pruned_or_retained_branch(
        block_number in any::<u64>(),
        pruned_through in any::<u64>(),
        retained in 0u64..4,
        returned_ok in 0u64..4,
        returned_pruned in 0u64..4,
    ) {
        let canonical = is_bit(retained) && is_bit(returned_ok) && is_bit(returned_pruned);
        let pruned_branch = block_number <= pruned_through
            && retained == 0
            && returned_ok == 0
            && returned_pruned == 1;
        let retained_branch = pruned_through < block_number
            && returned_pruned == 0
            && retained == returned_ok;
        let expected = canonical && (pruned_branch || retained_branch);

        prop_assert_eq!(
            prune_query_observation_safe_raw(
                block_number,
                pruned_through,
                retained,
                returned_ok,
                returned_pruned,
            ),
            expected
        );
    }

    #[test]
    fn pbt_prune_partial_progress_preserves_restartable_cursor_invariants(
        previous_present in 0u64..4,
        previous_boundary in any::<u64>(),
        next_present in 0u64..4,
        next_boundary in any::<u64>(),
        next_cursor in any::<u64>(),
        max_ops in any::<u64>(),
        ops_used in any::<u64>(),
        next_ops_needed in any::<u64>(),
        did_work in 0u64..4,
        stopped_for_budget in 0u64..4,
    ) {
        let input = ProgressInput {
            previous_present,
            previous_boundary,
            next_present,
            next_boundary,
            next_cursor,
            max_ops,
            ops_used,
            next_ops_needed,
            did_work,
            stopped_for_budget,
        };
        let result = prune_partial_progress_safe_raw(
            input.previous_present,
            input.previous_boundary,
            input.next_present,
            input.next_boundary,
            input.next_cursor,
            input.max_ops,
            input.ops_used,
            input.next_ops_needed,
            input.did_work,
            input.stopped_for_budget,
        );

        prop_assert_eq!(result, expected_prune_progress(input));
        if result {
            prop_assert!(ops_used <= max_ops);
            prop_assert!(did_work == 0 || next_present == 1);
            prop_assert!(previous_present == 0 || next_present == 1);
            prop_assert!(next_present == 0 || next_boundary < next_cursor);
            prop_assert!(stopped_for_budget == 0 || next_present == 1);
        }
    }
}
