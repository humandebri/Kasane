//! どこで: wrap request 境界 / 何を: pending・idempotency・stage・recover条件 / なぜ: 二重請求と不正復旧を防ぐため
#![allow(clippy::too_many_arguments)]

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_REQUEST_STATUS_QUEUED: u64 = 0;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_REQUEST_STATUS_RUNNING: u64 = 1;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_REQUEST_STATUS_SUCCEEDED: u64 = 2;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_REQUEST_STATUS_FAILED: u64 = 3;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_STAGE_FEE_PENDING: u64 = 0;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_STAGE_FEE_COLLECTED: u64 = 1;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_STAGE_PULL_PENDING: u64 = 2;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_STAGE_PULLED: u64 = 3;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_STAGE_MINT_SUBMITTING: u64 = 4;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_STAGE_MINT_SUBMITTED: u64 = 5;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_STAGE_SUCCEEDED: u64 = 6;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_STAGE_FAILED: u64 = 7;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_STAGE_REFUNDING: u64 = 8;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_STAGE_REFUNDED: u64 = 9;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_RESERVE_OK: u64 = 0;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_RESERVE_IN_PROGRESS: u64 = 1;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_RESERVE_IDEMPOTENCY_MISMATCH: u64 = 2;

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result == (
        (existing_present == 0
            && inserted == 1
            && result_code == WRAP_RESERVE_OK)
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
    ),
))]
pub fn wrap_pending_reservation_safe_raw(
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

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result == (
        (fields_match == 1
            && fee_ledger_tx_id_present == 1
            && charged_fee_present == 1
            && charged_gas_price_present == 1
            && returns_ok == 1
            && mismatch_error == 0
            && incomplete_error == 0)
        || (fields_match == 0
            && returns_ok == 0
            && mismatch_error == 1
            && incomplete_error == 0)
        || (fields_match == 1
            && (fee_ledger_tx_id_present == 0
                || charged_fee_present == 0
                || charged_gas_price_present == 0)
            && returns_ok == 0
            && mismatch_error == 0
            && incomplete_error == 1)
    ),
))]
pub fn wrap_idempotent_response_safe_raw(
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

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result == (
        (previous_stage == WRAP_STAGE_FEE_PENDING
            && (next_stage == WRAP_STAGE_FEE_PENDING
                || next_stage == WRAP_STAGE_FEE_COLLECTED
                || next_stage == WRAP_STAGE_FAILED))
        || (previous_stage == WRAP_STAGE_FEE_COLLECTED
            && (next_stage == WRAP_STAGE_FEE_COLLECTED
                || next_stage == WRAP_STAGE_PULL_PENDING
                || next_stage == WRAP_STAGE_PULLED
                || next_stage == WRAP_STAGE_MINT_SUBMITTING
                || next_stage == WRAP_STAGE_FAILED))
        || (previous_stage == WRAP_STAGE_PULL_PENDING
            && (next_stage == WRAP_STAGE_PULL_PENDING
                || next_stage == WRAP_STAGE_PULLED
                || next_stage == WRAP_STAGE_FAILED))
        || (previous_stage == WRAP_STAGE_PULLED
            && (next_stage == WRAP_STAGE_PULLED
                || next_stage == WRAP_STAGE_MINT_SUBMITTING
                || next_stage == WRAP_STAGE_FAILED))
        || (previous_stage == WRAP_STAGE_MINT_SUBMITTING
            && (next_stage == WRAP_STAGE_MINT_SUBMITTING
                || next_stage == WRAP_STAGE_MINT_SUBMITTED
                || next_stage == WRAP_STAGE_SUCCEEDED
                || next_stage == WRAP_STAGE_FAILED))
        || (previous_stage == WRAP_STAGE_MINT_SUBMITTED
            && (next_stage == WRAP_STAGE_MINT_SUBMITTED
                || next_stage == WRAP_STAGE_SUCCEEDED
                || next_stage == WRAP_STAGE_FAILED))
        || (previous_stage == WRAP_STAGE_FAILED
            && ((next_stage == WRAP_STAGE_FAILED)
                || (recovering == 1 && next_stage == WRAP_STAGE_REFUNDING)))
        || (previous_stage == WRAP_STAGE_REFUNDING
            && (next_stage == WRAP_STAGE_REFUNDED || next_stage == WRAP_STAGE_FAILED))
        || (previous_stage == WRAP_STAGE_SUCCEEDED && next_stage == WRAP_STAGE_SUCCEEDED)
        || (previous_stage == WRAP_STAGE_REFUNDED && next_stage == WRAP_STAGE_REFUNDED)
    ),
))]
pub fn wrap_stage_transition_safe_raw(
    previous_stage: u64,
    next_stage: u64,
    recovering: u64,
) -> bool {
    (previous_stage == WRAP_STAGE_FEE_PENDING
        && (next_stage == WRAP_STAGE_FEE_PENDING
            || next_stage == WRAP_STAGE_FEE_COLLECTED
            || next_stage == WRAP_STAGE_FAILED))
        || (previous_stage == WRAP_STAGE_FEE_COLLECTED
            && (next_stage == WRAP_STAGE_FEE_COLLECTED
                || next_stage == WRAP_STAGE_PULL_PENDING
                || next_stage == WRAP_STAGE_PULLED
                || next_stage == WRAP_STAGE_MINT_SUBMITTING
                || next_stage == WRAP_STAGE_FAILED))
        || (previous_stage == WRAP_STAGE_PULL_PENDING
            && (next_stage == WRAP_STAGE_PULL_PENDING
                || next_stage == WRAP_STAGE_PULLED
                || next_stage == WRAP_STAGE_FAILED))
        || (previous_stage == WRAP_STAGE_PULLED
            && (next_stage == WRAP_STAGE_PULLED
                || next_stage == WRAP_STAGE_MINT_SUBMITTING
                || next_stage == WRAP_STAGE_FAILED))
        || (previous_stage == WRAP_STAGE_MINT_SUBMITTING
            && (next_stage == WRAP_STAGE_MINT_SUBMITTING
                || next_stage == WRAP_STAGE_MINT_SUBMITTED
                || next_stage == WRAP_STAGE_SUCCEEDED
                || next_stage == WRAP_STAGE_FAILED))
        || (previous_stage == WRAP_STAGE_MINT_SUBMITTED
            && (next_stage == WRAP_STAGE_MINT_SUBMITTED
                || next_stage == WRAP_STAGE_SUCCEEDED
                || next_stage == WRAP_STAGE_FAILED))
        || (previous_stage == WRAP_STAGE_FAILED
            && ((next_stage == WRAP_STAGE_FAILED)
                || (recovering == 1 && next_stage == WRAP_STAGE_REFUNDING)))
        || (previous_stage == WRAP_STAGE_REFUNDING
            && (next_stage == WRAP_STAGE_REFUNDED || next_stage == WRAP_STAGE_FAILED))
        || (previous_stage == WRAP_STAGE_SUCCEEDED && next_stage == WRAP_STAGE_SUCCEEDED)
        || (previous_stage == WRAP_STAGE_REFUNDED && next_stage == WRAP_STAGE_REFUNDED)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result == (
        status == WRAP_REQUEST_STATUS_FAILED
        && gas_limit_nonzero == 1
        && mint_failed_recoverable == 1
        && pull_ledger_tx_id_present == 1
        && mint_tx_id_present == 0
        && withdraw_in_progress == 0
        && withdrawn == 0
        && withdraw_ledger_tx_id_present == 0
    ),
))]
pub fn wrap_recover_allowed_raw(
    status: u64,
    gas_limit_nonzero: u64,
    mint_failed_recoverable: u64,
    pull_ledger_tx_id_present: u64,
    mint_tx_id_present: u64,
    withdraw_in_progress: u64,
    withdrawn: u64,
    withdraw_ledger_tx_id_present: u64,
) -> bool {
    status == WRAP_REQUEST_STATUS_FAILED
        && gas_limit_nonzero == 1
        && mint_failed_recoverable == 1
        && pull_ledger_tx_id_present == 1
        && mint_tx_id_present == 0
        && withdraw_in_progress == 0
        && withdrawn == 0
        && withdraw_ledger_tx_id_present == 0
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result == (
        gas_limit_zero == 1
        && mint_failed_recoverable == 1
        && status != WRAP_REQUEST_STATUS_RUNNING
        && pull_ledger_tx_id_present == 1
    ),
))]
pub fn native_deposit_retry_allowed_raw(
    gas_limit_zero: u64,
    status: u64,
    mint_failed_recoverable: u64,
    pull_ledger_tx_id_present: u64,
) -> bool {
    gas_limit_zero == 1
        && mint_failed_recoverable == 1
        && status != WRAP_REQUEST_STATUS_RUNNING
        && pull_ledger_tx_id_present == 1
}
