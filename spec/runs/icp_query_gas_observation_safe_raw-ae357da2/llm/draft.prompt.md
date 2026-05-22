Generate a concise spec draft candidate:
#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        observed_address_code == ICP_QUERY_PRECOMPILE_ADDRESS_CODE
        && returned_success <= 1
        && (input_len <= MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS
            && reply_len <= MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS
            ==> charged_gas >= ICP_QUERY_BASE_GAS
                + input_len * ICP_QUERY_INPUT_BYTE_GAS
                + reply_len * ICP_QUERY_REPLY_BYTE_GAS)
        && (returned_success == 1 ==> gas_limit >= charged_gas)
        && (returned_success == 0 ==> gas_limit < charged_gas)
    ),
))]
pub fn icp_query_gas_observation_safe_raw(
    observed_address_code: u64,
    input_len: u64,
    reply_len: u64,
    charged_gas: u64,
    gas_limit: u64,
    returned_success: u64,
) -> bool
{
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
