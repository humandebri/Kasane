//! どこで: ICP query compact input model / 何を: update kind拒否 / なぜ: specgen contractを単一targetファイルへ注入するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_PRECOMPILE_KIND_UPDATE: u64 = 1;

#[allow(dead_code)]
fn main() {}

pub fn icp_query_update_kind_rejected_raw(kind: u64) -> bool {
    kind == ICP_PRECOMPILE_KIND_UPDATE
}
