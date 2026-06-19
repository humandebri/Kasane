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
pub const MAX_ICP_QUERY_ARG_LEN: u64 = 3_997;
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
pub const ICP_QUERY_OUTCOME_RETURN: u64 = 1;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_QUERY_OUTCOME_OOG: u64 = 2;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_QUERY_OUTCOME_OTHER_FAILURE: u64 = 3;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_UPDATE_STATUS_QUEUED: u64 = 0;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_UPDATE_STATUS_DISPATCHING: u64 = 1;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_UPDATE_STATUS_DISPATCHED: u64 = 2;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_UPDATE_STATUS_DISPATCH_FAILED: u64 = 3;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_UPDATE_STATUS_DISPATCH_UNCERTAIN: u64 = 4;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const MAX_ICP_QUERY_INPUT_LEN_WITH_EXACT_GAS: u64 = 1_152_921_504_606_846_975;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const MAX_ICP_QUERY_REPLY_LEN_WITH_EXACT_GAS: u64 = 2_305_843_009_213_693_951;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS: u64 = 768_614_336_404_562_567;

mod compact_icp_query_input;
mod icp_precompile_allowlist_entry;
mod icp_query_execution_gate;
mod icp_query_gas_observation;
mod icp_query_update_kind_rejected;
mod icp_update_capacity_accepts;
mod icp_update_status_consumes_capacity;

pub use compact_icp_query_input::compact_icp_query_input_safe_raw;
pub use icp_precompile_allowlist_entry::icp_precompile_allowlist_entry_safe_raw;
pub use icp_query_execution_gate::icp_query_execution_gate_safe_raw;
pub use icp_query_gas_observation::icp_query_gas_observation_safe_raw;
pub use icp_query_update_kind_rejected::icp_query_update_kind_rejected_raw;
pub use icp_update_capacity_accepts::icp_update_capacity_accepts_raw;
pub use icp_update_status_consumes_capacity::icp_update_status_consumes_capacity_raw;

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
