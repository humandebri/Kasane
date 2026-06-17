//! どこで: Kasane precompile 観測境界 / 何を: compact入力・log shape・gas policy / なぜ: 不正intentと過小gasを防ぐため
#![allow(clippy::manual_range_contains, clippy::too_many_arguments)]

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WRAP_PRECOMPILE_ADDRESS_CODE: u64 = 1;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const NATIVE_WITHDRAW_PRECOMPILE_ADDRESS_CODE: u64 = 2;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_QUERY_PRECOMPILE_ADDRESS_CODE: u64 = 3;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const COMPACT_FORMAT_VERSION: u64 = 1;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const MAX_PRINCIPAL_LEN: u64 = 29;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const MAX_QUERY_METHOD_LEN: u64 = 64;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const COMPACT_UNWRAP_INPUT_LEN: u64 = 93;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const COMPACT_NATIVE_WITHDRAW_INPUT_LEN: u64 = 31;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const UNWRAP_BURN_GAS_SURCHARGE: u64 = 45_000;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_QUERY_KIND_QUERY: u64 = 0;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_PRECOMPILE_KIND_UPDATE: u64 = 1;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_QUERY_BASE_GAS: u64 = 50_000;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_QUERY_INPUT_BYTE_GAS: u64 = 16;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_QUERY_REPLY_BYTE_GAS: u64 = 8;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const MAX_ICP_QUERY_INPUT_LEN_WITH_EXACT_GAS: u64 = 1_152_921_504_606_846_975;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const MAX_ICP_QUERY_REPLY_LEN_WITH_EXACT_GAS: u64 = 2_305_843_009_213_693_951;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS: u64 = 768_614_336_404_562_567;

#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        len >= 1
        && len <= MAX_PRINCIPAL_LEN
        && slot_present == 1
        && padding_zero == 1
    ),
))]
pub fn compact_principal_slot_safe_raw(len: u64, slot_present: u64, padding_zero: u64) -> bool {
    len >= 1 && len <= MAX_PRINCIPAL_LEN && slot_present == 1 && padding_zero == 1
}

#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        input_len == COMPACT_UNWRAP_INPUT_LEN
        && version == COMPACT_FORMAT_VERSION
        && asset_len >= 1
        && asset_len <= MAX_PRINCIPAL_LEN
        && asset_slot_present == 1
        && asset_padding_zero == 1
        && amount_present == 1
        && recipient_len >= 1
        && recipient_len <= MAX_PRINCIPAL_LEN
        && recipient_slot_present == 1
        && recipient_padding_zero == 1
    ),
))]
pub fn compact_unwrap_input_safe_raw(
    input_len: u64,
    version: u64,
    asset_len: u64,
    asset_slot_present: u64,
    asset_padding_zero: u64,
    amount_present: u64,
    recipient_len: u64,
    recipient_slot_present: u64,
    recipient_padding_zero: u64,
) -> bool {
    input_len == COMPACT_UNWRAP_INPUT_LEN
        && version == COMPACT_FORMAT_VERSION
        && asset_len >= 1
        && asset_len <= MAX_PRINCIPAL_LEN
        && asset_slot_present == 1
        && asset_padding_zero == 1
        && amount_present == 1
        && recipient_len >= 1
        && recipient_len <= MAX_PRINCIPAL_LEN
        && recipient_slot_present == 1
        && recipient_padding_zero == 1
}

#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        input_len == COMPACT_NATIVE_WITHDRAW_INPUT_LEN
        && version == COMPACT_FORMAT_VERSION
        && recipient_len >= 1
        && recipient_len <= MAX_PRINCIPAL_LEN
        && recipient_slot_present == 1
        && recipient_padding_zero == 1
        && recipient_is_anonymous == 0
    ),
))]
pub fn compact_native_withdraw_input_safe_raw(
    input_len: u64,
    version: u64,
    recipient_len: u64,
    recipient_slot_present: u64,
    recipient_padding_zero: u64,
    recipient_is_anonymous: u64,
) -> bool {
    input_len == COMPACT_NATIVE_WITHDRAW_INPUT_LEN
        && version == COMPACT_FORMAT_VERSION
        && recipient_len >= 1
        && recipient_len <= MAX_PRINCIPAL_LEN
        && recipient_slot_present == 1
        && recipient_padding_zero == 1
        && recipient_is_anonymous == 0
}

#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        version == COMPACT_FORMAT_VERSION
        && kind == ICP_QUERY_KIND_QUERY
        && target_len >= 1
        && target_len <= MAX_PRINCIPAL_LEN
        && target_present == 1
        && method_len >= 1
        && method_len <= MAX_QUERY_METHOD_LEN
        && method_present == 1
        && method_utf8 == 1
        && arg_present == 1
        && consumed_exact == 1
    ),
))]
pub fn compact_icp_query_input_safe_raw(
    version: u64,
    kind: u64,
    target_len: u64,
    target_present: u64,
    method_len: u64,
    method_present: u64,
    method_utf8: u64,
    arg_present: u64,
    consumed_exact: u64,
) -> bool {
    version == COMPACT_FORMAT_VERSION
        && kind == ICP_QUERY_KIND_QUERY
        && target_len >= 1
        && target_len <= MAX_PRINCIPAL_LEN
        && target_present == 1
        && method_len >= 1
        && method_len <= MAX_QUERY_METHOD_LEN
        && method_present == 1
        && method_utf8 == 1
        && arg_present == 1
        && consumed_exact == 1
}

#[cfg_attr(verus_keep_ghost, verus_spec(rejected => ensures
    rejected == (kind == ICP_PRECOMPILE_KIND_UPDATE),
))]
pub fn icp_query_update_kind_rejected_raw(kind: u64) -> bool {
    kind == ICP_PRECOMPILE_KIND_UPDATE
}

#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        target_len >= 1
        && target_len <= MAX_PRINCIPAL_LEN
        && target_non_anonymous == 1
        && method_len >= 1
        && method_len <= MAX_QUERY_METHOD_LEN
        && method_ascii == 1
    ),
))]
pub fn icp_precompile_allowlist_entry_safe_raw(
    target_len: u64,
    target_non_anonymous: u64,
    method_len: u64,
    method_ascii: u64,
) -> bool {
    target_len >= 1
        && target_len <= MAX_PRINCIPAL_LEN
        && target_non_anonymous == 1
        && method_len >= 1
        && method_len <= MAX_QUERY_METHOD_LEN
        && method_ascii == 1
}

#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        address_code == expected_address_code
        && topic_count == 1
        && topic_matches == 1
        && data_consumed == 1
        && fields_valid == 1
    ),
))]
pub fn precompile_log_shape_safe_raw(
    address_code: u64,
    expected_address_code: u64,
    topic_count: u64,
    topic_matches: u64,
    data_consumed: u64,
    fields_valid: u64,
) -> bool {
    address_code == expected_address_code
        && topic_count == 1
        && topic_matches == 1
        && data_consumed == 1
        && fields_valid == 1
}

#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        (address_code == WRAP_PRECOMPILE_ADDRESS_CODE && extra_gas == 0)
        || (address_code != WRAP_PRECOMPILE_ADDRESS_CODE
            && extra_gas == ratio_extra_gas)
    ),
))]
pub fn precompile_extra_gas_policy_safe_raw(
    address_code: u64,
    ratio_extra_gas: u64,
    extra_gas: u64,
) -> bool {
    (address_code == WRAP_PRECOMPILE_ADDRESS_CODE && extra_gas == 0)
        || (address_code != WRAP_PRECOMPILE_ADDRESS_CODE && extra_gas == ratio_extra_gas)
}

#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        gas_a >= UNWRAP_BURN_GAS_SURCHARGE
        && gas_b >= UNWRAP_BURN_GAS_SURCHARGE
        && (input_len_a <= input_len_b
            && log_data_len_a <= log_data_len_b
            && field_count_a <= field_count_b
            ==> gas_a <= gas_b)
    ),
))]
pub fn wrap_precompile_gas_observation_safe_raw(
    input_len_a: u64,
    log_data_len_a: u64,
    field_count_a: u64,
    gas_a: u64,
    input_len_b: u64,
    log_data_len_b: u64,
    field_count_b: u64,
    gas_b: u64,
) -> bool {
    gas_a >= UNWRAP_BURN_GAS_SURCHARGE
        && gas_b >= UNWRAP_BURN_GAS_SURCHARGE
        && ((input_len_a > input_len_b
            || log_data_len_a > log_data_len_b
            || field_count_a > field_count_b)
            || gas_a <= gas_b)
}

#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        observed_address_code == ICP_QUERY_PRECOMPILE_ADDRESS_CODE
        && returned_success <= 1
        && (input_len > MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS
            || reply_len > MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS
            || charged_gas >= ICP_QUERY_BASE_GAS
                + input_len * ICP_QUERY_INPUT_BYTE_GAS
                + reply_len * ICP_QUERY_REPLY_BYTE_GAS)
        && (returned_success != 1 || gas_limit >= charged_gas)
        && (returned_success != 0 || gas_limit < charged_gas)
    ),
))]
pub fn icp_query_gas_observation_safe_raw(
    observed_address_code: u64,
    input_len: u64,
    reply_len: u64,
    charged_gas: u64,
    gas_limit: u64,
    returned_success: u64,
) -> bool {
    let exact_combined_len = input_len <= MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS
        && reply_len <= MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS;
    let exact_charged_gas = if exact_combined_len {
        let input_gas = input_len * ICP_QUERY_INPUT_BYTE_GAS;
        let reply_gas = reply_len * ICP_QUERY_REPLY_BYTE_GAS;
        charged_gas >= ICP_QUERY_BASE_GAS + input_gas + reply_gas
    } else {
        true
    };
    observed_address_code == ICP_QUERY_PRECOMPILE_ADDRESS_CODE
        && returned_success <= 1
        && exact_charged_gas
        && (returned_success != 1 || gas_limit >= charged_gas)
        && (returned_success != 0 || gas_limit < charged_gas)
}

#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        calls_before == 0
        && mode_allows_external == 1
        && value_is_zero == 1
        && parsed_input == 1
    ),
))]
pub fn icp_query_execution_gate_safe_raw(
    calls_before: u64,
    mode_allows_external: u64,
    value_is_zero: u64,
    parsed_input: u64,
) -> bool {
    calls_before == 0 && mode_allows_external == 1 && value_is_zero == 1 && parsed_input == 1
}
