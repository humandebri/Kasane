Generate scenario candidates:
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
) -> bool
{
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
