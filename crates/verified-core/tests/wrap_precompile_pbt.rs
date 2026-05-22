//! どこで: wrap precompile PBT / 何を: compact入力・log・gas policy / なぜ: intent decode仕様の取り違えを検出するため

use proptest::prelude::*;
use verified_core::wrap_precompile::{
    compact_native_withdraw_input_safe_raw, compact_principal_slot_safe_raw,
    compact_unwrap_input_safe_raw, precompile_extra_gas_policy_safe_raw,
    precompile_log_shape_safe_raw, wrap_precompile_gas_observation_safe_raw,
    COMPACT_FORMAT_VERSION, COMPACT_NATIVE_WITHDRAW_INPUT_LEN, COMPACT_UNWRAP_INPUT_LEN,
    MAX_PRINCIPAL_LEN, NATIVE_WITHDRAW_PRECOMPILE_ADDRESS_CODE, UNWRAP_BURN_GAS_SURCHARGE,
    WRAP_PRECOMPILE_ADDRESS_CODE,
};

fn expected_principal(len: u64, slot_present: u64, padding_zero: u64) -> bool {
    (1..=MAX_PRINCIPAL_LEN).contains(&len) && slot_present == 1 && padding_zero == 1
}

fn expected_ratio_extra(elapsed_instruction: u64, numerator: u64, denominator: u64) -> u64 {
    if elapsed_instruction == 0 || numerator == 0 {
        return 0;
    }
    let denominator = denominator.max(1);
    let scaled = u128::from(elapsed_instruction).saturating_mul(u128::from(numerator));
    let rounded =
        scaled.saturating_add(u128::from(denominator).saturating_sub(1)) / u128::from(denominator);
    rounded.min(u128::from(u64::MAX)) as u64
}

proptest! {
    #[test]
    fn pbt_compact_principal_and_unwrap_input_require_fixed_shape(
        input_len in 0u64..140,
        version in 0u64..4,
        asset_len in 0u64..40,
        asset_slot_present in 0u64..3,
        asset_padding_zero in 0u64..3,
        amount_present in 0u64..3,
        recipient_len in 0u64..40,
        recipient_slot_present in 0u64..3,
        recipient_padding_zero in 0u64..3,
    ) {
        prop_assert_eq!(
            compact_principal_slot_safe_raw(asset_len, asset_slot_present, asset_padding_zero),
            expected_principal(asset_len, asset_slot_present, asset_padding_zero)
        );
        prop_assert_eq!(
            compact_unwrap_input_safe_raw(
                input_len,
                version,
                asset_len,
                asset_slot_present,
                asset_padding_zero,
                amount_present,
                recipient_len,
                recipient_slot_present,
                recipient_padding_zero,
            ),
            input_len == COMPACT_UNWRAP_INPUT_LEN
                && version == COMPACT_FORMAT_VERSION
                && expected_principal(asset_len, asset_slot_present, asset_padding_zero)
                && amount_present == 1
                && expected_principal(
                    recipient_len,
                    recipient_slot_present,
                    recipient_padding_zero,
                )
        );
    }

    #[test]
    fn pbt_native_withdraw_input_rejects_anonymous_principal(
        input_len in 0u64..80,
        version in 0u64..4,
        recipient_len in 0u64..40,
        recipient_slot_present in 0u64..3,
        recipient_padding_zero in 0u64..3,
        recipient_is_anonymous in 0u64..3,
    ) {
        prop_assert_eq!(
            compact_native_withdraw_input_safe_raw(
                input_len,
                version,
                recipient_len,
                recipient_slot_present,
                recipient_padding_zero,
                recipient_is_anonymous,
            ),
            input_len == COMPACT_NATIVE_WITHDRAW_INPUT_LEN
                && version == COMPACT_FORMAT_VERSION
                && expected_principal(
                    recipient_len,
                    recipient_slot_present,
                    recipient_padding_zero,
                )
                && recipient_is_anonymous == 0
        );
    }

    #[test]
    fn pbt_precompile_log_shape_requires_address_topic_and_complete_data(
        address_code in 0u64..4,
        expected_address_code in prop_oneof![
            Just(WRAP_PRECOMPILE_ADDRESS_CODE),
            Just(NATIVE_WITHDRAW_PRECOMPILE_ADDRESS_CODE),
        ],
        topic_count in 0u64..4,
        topic_matches in 0u64..3,
        data_consumed in 0u64..3,
        fields_valid in 0u64..3,
    ) {
        prop_assert_eq!(
            precompile_log_shape_safe_raw(
                address_code,
                expected_address_code,
                topic_count,
                topic_matches,
                data_consumed,
                fields_valid,
            ),
            address_code == expected_address_code
                && topic_count == 1
                && topic_matches == 1
                && data_consumed == 1
                && fields_valid == 1
        );
    }

    #[test]
    fn pbt_precompile_extra_gas_policy_exempts_wrap_address_only(
        address_code in 0u64..4,
        elapsed_instruction in any::<u64>(),
        numerator in any::<u64>(),
        denominator in any::<u64>(),
        extra_gas in any::<u64>(),
    ) {
        let expected_extra = expected_ratio_extra(elapsed_instruction, numerator, denominator);
        prop_assert_eq!(
            precompile_extra_gas_policy_safe_raw(
                address_code,
                expected_extra,
                extra_gas,
            ),
            (address_code == WRAP_PRECOMPILE_ADDRESS_CODE && extra_gas == 0)
                || (address_code != WRAP_PRECOMPILE_ADDRESS_CODE && extra_gas == expected_extra)
        );
    }

    #[test]
    fn pbt_wrap_precompile_gas_observation_preserves_floor_and_monotonicity(
        input_len_a in any::<u64>(),
        log_data_len_a in any::<u64>(),
        field_count_a in any::<u64>(),
        gas_a in any::<u64>(),
        input_len_b in any::<u64>(),
        log_data_len_b in any::<u64>(),
        field_count_b in any::<u64>(),
        gas_b in any::<u64>(),
    ) {
        prop_assert_eq!(
            wrap_precompile_gas_observation_safe_raw(
                input_len_a,
                log_data_len_a,
                field_count_a,
                gas_a,
                input_len_b,
                log_data_len_b,
                field_count_b,
                gas_b,
            ),
            gas_a >= UNWRAP_BURN_GAS_SURCHARGE
                && gas_b >= UNWRAP_BURN_GAS_SURCHARGE
                && ((input_len_a > input_len_b
                    || log_data_len_a > log_data_len_b
                    || field_count_a > field_count_b)
                    || gas_a <= gas_b)
        );
    }
}
