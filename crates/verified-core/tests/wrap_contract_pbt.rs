//! どこで: wrap request PBT / 何を: idempotency・stage・recover / なぜ: 二重請求と不正復旧仕様の誤りを検出するため

use proptest::prelude::*;
use verified_core::wrap_request::{
    wrap_idempotent_response_safe_raw, wrap_pending_reservation_safe_raw, wrap_recover_allowed_raw,
    wrap_stage_transition_safe_raw, WRAP_REQUEST_STATUS_FAILED, WRAP_RESERVE_IDEMPOTENCY_MISMATCH,
    WRAP_RESERVE_IN_PROGRESS, WRAP_RESERVE_OK, WRAP_STAGE_FAILED, WRAP_STAGE_FEE_COLLECTED,
    WRAP_STAGE_FEE_PENDING, WRAP_STAGE_MINT_SUBMITTED, WRAP_STAGE_MINT_SUBMITTING,
    WRAP_STAGE_PULLED, WRAP_STAGE_PULL_PENDING, WRAP_STAGE_REFUNDED, WRAP_STAGE_REFUNDING,
    WRAP_STAGE_SUCCEEDED,
};

fn expected_reservation(
    existing_present: u64,
    existing_decode_placeholder: u64,
    request_id_matches: u64,
    caller_matches: u64,
    inserted: u64,
    result_code: u64,
) -> bool {
    (existing_present == 0 && inserted == 1 && result_code == WRAP_RESERVE_OK)
        || (existing_present == 1
            && existing_decode_placeholder == 1
            && inserted == 1
            && result_code == WRAP_RESERVE_OK)
        || (existing_present == 1
            && existing_decode_placeholder == 0
            && request_id_matches == 0
            && inserted == 1
            && result_code == WRAP_RESERVE_OK)
        || (existing_present == 1
            && existing_decode_placeholder == 0
            && request_id_matches == 1
            && caller_matches == 1
            && inserted == 0
            && result_code == WRAP_RESERVE_IN_PROGRESS)
        || (existing_present == 1
            && existing_decode_placeholder == 0
            && request_id_matches == 1
            && caller_matches == 0
            && inserted == 0
            && result_code == WRAP_RESERVE_IDEMPOTENCY_MISMATCH)
}

fn expected_idempotent(
    fields_match: u64,
    fee_ledger_tx_id_present: u64,
    charged_fee_present: u64,
    charged_gas_price_present: u64,
    returns_ok: u64,
    mismatch_error: u64,
    incomplete_error: u64,
) -> bool {
    (fields_match == 1
        && fee_ledger_tx_id_present == 1
        && charged_fee_present == 1
        && charged_gas_price_present == 1
        && returns_ok == 1
        && mismatch_error == 0
        && incomplete_error == 0)
        || (fields_match == 0 && returns_ok == 0 && mismatch_error == 1 && incomplete_error == 0)
        || (fields_match == 1
            && (fee_ledger_tx_id_present == 0
                || charged_fee_present == 0
                || charged_gas_price_present == 0)
            && returns_ok == 0
            && mismatch_error == 0
            && incomplete_error == 1)
}

fn expected_stage(previous_stage: u64, next_stage: u64, recovering: u64) -> bool {
    match previous_stage {
        WRAP_STAGE_FEE_PENDING => matches!(
            next_stage,
            WRAP_STAGE_FEE_PENDING | WRAP_STAGE_FEE_COLLECTED | WRAP_STAGE_FAILED
        ),
        WRAP_STAGE_FEE_COLLECTED => matches!(
            next_stage,
            WRAP_STAGE_FEE_COLLECTED
                | WRAP_STAGE_PULL_PENDING
                | WRAP_STAGE_PULLED
                | WRAP_STAGE_MINT_SUBMITTING
                | WRAP_STAGE_FAILED
        ),
        WRAP_STAGE_PULL_PENDING => matches!(
            next_stage,
            WRAP_STAGE_PULL_PENDING | WRAP_STAGE_PULLED | WRAP_STAGE_FAILED
        ),
        WRAP_STAGE_PULLED => {
            matches!(
                next_stage,
                WRAP_STAGE_PULLED | WRAP_STAGE_MINT_SUBMITTING | WRAP_STAGE_FAILED
            )
        }
        WRAP_STAGE_MINT_SUBMITTING => matches!(
            next_stage,
            WRAP_STAGE_MINT_SUBMITTING
                | WRAP_STAGE_MINT_SUBMITTED
                | WRAP_STAGE_SUCCEEDED
                | WRAP_STAGE_FAILED
        ),
        WRAP_STAGE_MINT_SUBMITTED => matches!(
            next_stage,
            WRAP_STAGE_MINT_SUBMITTED | WRAP_STAGE_SUCCEEDED | WRAP_STAGE_FAILED
        ),
        WRAP_STAGE_FAILED => {
            next_stage == WRAP_STAGE_FAILED
                || (recovering == 1 && next_stage == WRAP_STAGE_REFUNDING)
        }
        WRAP_STAGE_REFUNDING => {
            next_stage == WRAP_STAGE_REFUNDED || next_stage == WRAP_STAGE_FAILED
        }
        WRAP_STAGE_SUCCEEDED => next_stage == WRAP_STAGE_SUCCEEDED,
        WRAP_STAGE_REFUNDED => next_stage == WRAP_STAGE_REFUNDED,
        _ => false,
    }
}

#[test]
fn wrap_stage_transition_rejects_terminal_and_skip_edges() {
    for (from, to, recovering) in [
        (WRAP_STAGE_FEE_PENDING, WRAP_STAGE_SUCCEEDED, 0),
        (WRAP_STAGE_FEE_PENDING, WRAP_STAGE_REFUNDED, 0),
        (WRAP_STAGE_SUCCEEDED, WRAP_STAGE_MINT_SUBMITTING, 0),
        (WRAP_STAGE_REFUNDED, WRAP_STAGE_FEE_COLLECTED, 0),
        (WRAP_STAGE_FAILED, WRAP_STAGE_REFUNDING, 0),
    ] {
        assert!(!wrap_stage_transition_safe_raw(from, to, recovering));
    }
}

#[test]
fn wrap_stage_transition_accepts_observed_edges() {
    for (from, to, recovering) in [
        (WRAP_STAGE_FEE_PENDING, WRAP_STAGE_FEE_COLLECTED, 0),
        (WRAP_STAGE_FEE_COLLECTED, WRAP_STAGE_PULL_PENDING, 0),
        (WRAP_STAGE_PULLED, WRAP_STAGE_MINT_SUBMITTING, 0),
        (WRAP_STAGE_MINT_SUBMITTED, WRAP_STAGE_SUCCEEDED, 0),
        (WRAP_STAGE_FAILED, WRAP_STAGE_REFUNDING, 1),
        (WRAP_STAGE_REFUNDING, WRAP_STAGE_REFUNDED, 0),
    ] {
        assert!(wrap_stage_transition_safe_raw(from, to, recovering));
    }
}

proptest! {
    #[test]
    fn pbt_wrap_pending_reservation_matches_idempotency_policy(
        existing_present in 0u64..3,
        existing_decode_placeholder in 0u64..3,
        request_id_matches in 0u64..3,
        caller_matches in 0u64..3,
        inserted in 0u64..3,
        result_code in 0u64..5,
    ) {
        prop_assert_eq!(
            wrap_pending_reservation_safe_raw(
                existing_present,
                existing_decode_placeholder,
                request_id_matches,
                caller_matches,
                inserted,
                result_code,
            ),
            expected_reservation(
                existing_present,
                existing_decode_placeholder,
                request_id_matches,
                caller_matches,
                inserted,
                result_code,
            )
        );
    }

    #[test]
    fn pbt_wrap_idempotent_response_requires_all_matching_fields_and_fee_outputs(
        fields_match in 0u64..3,
        fee_ledger_tx_id_present in 0u64..3,
        charged_fee_present in 0u64..3,
        charged_gas_price_present in 0u64..3,
        returns_ok in 0u64..3,
        mismatch_error in 0u64..3,
        incomplete_error in 0u64..3,
    ) {
        prop_assert_eq!(
            wrap_idempotent_response_safe_raw(
                fields_match,
                fee_ledger_tx_id_present,
                charged_fee_present,
                charged_gas_price_present,
                returns_ok,
                mismatch_error,
                incomplete_error,
            ),
            expected_idempotent(
                fields_match,
                fee_ledger_tx_id_present,
                charged_fee_present,
                charged_gas_price_present,
                returns_ok,
                mismatch_error,
                incomplete_error,
            )
        );
    }

    #[test]
    fn pbt_wrap_stage_and_recover_rules_do_not_reactivate_terminal_states(
        previous_stage in 0u64..12,
        next_stage in 0u64..12,
        recovering in 0u64..3,
        status in 0u64..5,
        gas_limit_nonzero in 0u64..3,
        mint_failed_recoverable in 0u64..3,
        pull_ledger_tx_id_present in 0u64..3,
        mint_tx_id_present in 0u64..3,
        withdraw_in_progress in 0u64..3,
        withdrawn in 0u64..3,
        withdraw_ledger_tx_id_present in 0u64..3,
    ) {
        prop_assert_eq!(
            wrap_stage_transition_safe_raw(previous_stage, next_stage, recovering),
            expected_stage(previous_stage, next_stage, recovering)
        );
        prop_assert_eq!(
            wrap_recover_allowed_raw(
                status,
                gas_limit_nonzero,
                mint_failed_recoverable,
                pull_ledger_tx_id_present,
                mint_tx_id_present,
                withdraw_in_progress,
                withdrawn,
                withdraw_ledger_tx_id_present,
            ),
            status == WRAP_REQUEST_STATUS_FAILED
                && gas_limit_nonzero == 1
                && mint_failed_recoverable == 1
                && pull_ledger_tx_id_present == 1
                && mint_tx_id_present == 0
                && withdraw_in_progress == 0
                && withdrawn == 0
                && withdraw_ledger_tx_id_present == 0
        );
    }

}
