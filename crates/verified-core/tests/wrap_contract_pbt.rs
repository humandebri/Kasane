//! どこで: wrap request PBT / 何を: idempotency・stage・quote・native金額 / なぜ: 二重請求と不正復旧仕様の誤りを検出するため

use proptest::prelude::*;
use verified_core::native_amount::{
    native_withdraw_amount_safe_raw, native_withdraw_receive_amount,
};
use verified_core::wrap_quote::{
    wrap_quote_approval_safe_raw, wrap_quote_components_safe_raw, GAS_PRICE_DENOMINATOR_BPS,
    WEI_PER_E8S,
};
use verified_core::wrap_request::{
    native_deposit_retry_allowed_raw, wrap_idempotent_response_safe_raw,
    wrap_pending_reservation_safe_raw, wrap_recover_allowed_raw, wrap_stage_transition_safe_raw,
    WRAP_REQUEST_STATUS_FAILED, WRAP_REQUEST_STATUS_RUNNING, WRAP_RESERVE_IDEMPOTENCY_MISMATCH,
    WRAP_RESERVE_IN_PROGRESS, WRAP_RESERVE_OK, WRAP_STAGE_FAILED, WRAP_STAGE_REFUNDED,
    WRAP_STAGE_REFUNDING, WRAP_STAGE_SUCCEEDED,
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
    (previous_stage < WRAP_STAGE_SUCCEEDED && previous_stage <= next_stage)
        || (previous_stage == WRAP_STAGE_FAILED
            && ((next_stage == WRAP_STAGE_FAILED)
                || (recovering == 1 && next_stage == WRAP_STAGE_REFUNDING)))
        || (previous_stage == WRAP_STAGE_REFUNDING
            && (next_stage == WRAP_STAGE_REFUNDED || next_stage == WRAP_STAGE_FAILED))
        || (previous_stage == WRAP_STAGE_SUCCEEDED && next_stage == WRAP_STAGE_SUCCEEDED)
        || (previous_stage == WRAP_STAGE_REFUNDED && next_stage == WRAP_STAGE_REFUNDED)
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

    #[test]
    fn pbt_native_deposit_retry_and_withdraw_amount_are_exact(
        gas_limit_zero in 0u64..3,
        status in 0u64..5,
        mint_failed_recoverable in 0u64..3,
        pull_ledger_tx_id_present in 0u64..3,
        amount_e8s in any::<u128>(),
        ledger_fee_e8s in any::<u128>(),
        receive_present in 0u64..3,
        receive_e8s in any::<u128>(),
    ) {
        prop_assert_eq!(
            native_deposit_retry_allowed_raw(
                gas_limit_zero,
                status,
                mint_failed_recoverable,
                pull_ledger_tx_id_present,
            ),
            gas_limit_zero == 1
                && mint_failed_recoverable == 1
                && status != WRAP_REQUEST_STATUS_RUNNING
                && pull_ledger_tx_id_present == 1
        );
        prop_assert_eq!(
            native_withdraw_receive_amount(amount_e8s, ledger_fee_e8s),
            amount_e8s.checked_sub(ledger_fee_e8s)
        );
        prop_assert_eq!(
            native_withdraw_amount_safe_raw(
                amount_e8s,
                ledger_fee_e8s,
                receive_present,
                receive_e8s,
            ),
            (amount_e8s >= ledger_fee_e8s
                && receive_present == 1
                && receive_e8s == amount_e8s - ledger_fee_e8s)
                || (amount_e8s < ledger_fee_e8s && receive_present == 0)
        );
    }

    #[test]
    fn pbt_wrap_quote_approval_and_components_are_exact(
        ledger_matches in 0u64..3,
        charged_fee_e8s in any::<u128>(),
        max_fee_e8s in any::<u128>(),
        charged_gas_price_wei in any::<u128>(),
        quoted_gas_price_wei in any::<u128>(),
        base_gas_price_wei in any::<u128>(),
        gas_price_buffer_bps in any::<u64>(),
        gas_limit in any::<u64>(),
        cycle_fee_e8s in any::<u64>(),
        gas_fee_e8s in any::<u128>(),
    ) {
        prop_assert_eq!(
            wrap_quote_approval_safe_raw(
                ledger_matches,
                charged_fee_e8s,
                max_fee_e8s,
                charged_gas_price_wei,
                quoted_gas_price_wei,
            ),
            ledger_matches == 1
                && charged_fee_e8s <= max_fee_e8s
                && charged_gas_price_wei <= quoted_gas_price_wei
        );

        let expected_gas_price = base_gas_price_wei
            .saturating_mul(u128::from(gas_price_buffer_bps))
            .saturating_add(GAS_PRICE_DENOMINATOR_BPS - 1)
            / GAS_PRICE_DENOMINATOR_BPS;
        let expected_gas_fee = charged_gas_price_wei
            .saturating_mul(u128::from(gas_limit))
            .saturating_add(WEI_PER_E8S - 1)
            / WEI_PER_E8S;
        let expected_charged_fee = gas_fee_e8s.saturating_add(u128::from(cycle_fee_e8s));
        let gas_price_component_matches = u64::from(charged_gas_price_wei == expected_gas_price);
        let gas_fee_component_matches = u64::from(gas_fee_e8s == expected_gas_fee);
        let charged_fee_component_matches = u64::from(charged_fee_e8s == expected_charged_fee);
        prop_assert_eq!(
            wrap_quote_components_safe_raw(
                gas_price_component_matches,
                gas_fee_component_matches,
                charged_fee_component_matches,
            ),
            gas_price_component_matches == 1
                && gas_fee_component_matches == 1
                && charged_fee_component_matches == 1
        );
    }
}
