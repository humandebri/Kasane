//! どこで: ICP query compact input model / 何を: update kind拒否 / なぜ: specgen contractを単一targetファイルへ注入するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_PRECOMPILE_KIND_UPDATE: u64 = 1;

#[allow(dead_code)]
fn main() {}

// specgen:contract icp_query_update_kind_rejected_raw-b2b79d8e 4c0815677d8f2031095adf41352b727e050382070d4ccb0a7deb6bb84503a233
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (kind == ICP_PRECOMPILE_KIND_UPDATE),
))]
pub fn icp_query_update_kind_rejected_raw(kind: u64) -> bool {
    kind == ICP_PRECOMPILE_KIND_UPDATE
}
