Review as implementation, edge-case, adversarial, Verus:
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
) -> bool
{
    target_len >= 1
        && target_len <= MAX_PRINCIPAL_LEN
        && target_non_anonymous == 1
        && method_len >= 1
        && method_len <= MAX_QUERY_METHOD_LEN
        && method_ascii == 1
}
