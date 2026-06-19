//! どこで: ICP update capacity model / 何を: active/reserved/journaled gate / なぜ: specgen contractを単一targetファイルへ注入するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[allow(dead_code)]
fn main() {}

pub fn icp_update_capacity_accepts_raw(
    existing_active: u64,
    reserved: u64,
    journaled: u64,
    max: u64,
) -> bool {
    existing_active < max
        && reserved <= max - existing_active
        && journaled < max - existing_active - reserved
}
