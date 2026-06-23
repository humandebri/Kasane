//! どこで: ICP update capacity model / 何を: status別capacity消費 / なぜ: specgen contractを単一targetファイルへ注入するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_UPDATE_STATUS_QUEUED: u64 = 0;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_UPDATE_STATUS_DISPATCHING: u64 = 1;

#[allow(dead_code)]
fn main() {}

// specgen:contract icp_update_status_consumes_capacity_raw-882a4379 7997881f9a9363d2153a04db673f2a2eb8db1ce8d0a270d6c012711175aa1b48
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (status_code == ICP_UPDATE_STATUS_QUEUED || status_code == ICP_UPDATE_STATUS_DISPATCHING),
))]
pub fn icp_update_status_consumes_capacity_raw(status_code: u64) -> bool {
    status_code == ICP_UPDATE_STATUS_QUEUED || status_code == ICP_UPDATE_STATUS_DISPATCHING
}
