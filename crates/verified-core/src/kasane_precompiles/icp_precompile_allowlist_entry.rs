//! どこで: ICP precompile allowlist model / 何を: allowlist entry境界条件 / なぜ: specgen contractを単一targetファイルへ注入するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const MAX_PRINCIPAL_LEN: u64 = 29;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const MAX_QUERY_METHOD_LEN: u64 = 64;

#[allow(dead_code)]
fn main() {}

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
