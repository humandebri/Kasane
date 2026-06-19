//! どこで: ICP update capacity model / 何を: active/reserved/journaled gate / なぜ: specgen contractを単一targetファイルへ注入するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[allow(dead_code)]
fn main() {}

// specgen:contract icp_update_capacity_accepts_raw-9d22db3f 53970715e653b9d36c2b46b91f381cd55e29c51611b3731584ffbe32381b2664
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (existing_active < max && reserved <= max - existing_active && journaled < max - existing_active - reserved),
))]
pub fn icp_update_capacity_accepts_raw(
    existing_active: u64,
    reserved: u64,
    journaled: u64,
    max: u64,
) -> bool
{
    existing_active < max
        && reserved <= max - existing_active
        && journaled < max - existing_active - reserved
}
