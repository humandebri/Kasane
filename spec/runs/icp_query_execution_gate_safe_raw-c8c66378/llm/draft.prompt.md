Generate a concise spec draft candidate:
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
) -> bool
{
    calls_before == 0 && mode_allows_external == 1 && value_is_zero == 1 && parsed_input == 1
}
