//! どこで: ICP query compact input model / 何を: parser境界条件 / なぜ: specgen contractを単一targetファイルへ注入するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const COMPACT_FORMAT_VERSION: u64 = 1;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_QUERY_KIND_QUERY: u64 = 0;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const MAX_PRINCIPAL_LEN: u64 = 29;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const MAX_QUERY_METHOD_LEN: u64 = 64;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const MAX_ICP_QUERY_ARG_LEN: u64 = 3_997;

#[allow(dead_code)]
fn main() {}

pub fn compact_icp_query_input_safe_raw(
    version: u64,
    kind: u64,
    target_len: u64,
    target_present: u64,
    method_len: u64,
    method_present: u64,
    method_utf8: u64,
    arg_present: u64,
    arg_len: u64,
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
        && arg_len <= MAX_ICP_QUERY_ARG_LEN
        && consumed_exact == 1
}
