//! どこで: EVM canister block commit / 何を: head・gas・batch整合性 / なぜ: specgen contractを単一targetファイルへ注入するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

// specgen:contract block_commit_safe_raw-318a0bf6 0adfa2894d689d1bc6f626938a945f356537df1222b40b95f8b71a16a8e61333
pub fn block_commit_safe_raw(
    previous_head: u64,
    committed_head: u64,
    included_count: u64,
    staged_count: u64,
    safe_included_count: u64,
    block_gas_used: u64,
    block_gas_limit: u64,
) -> (result: bool)
    requires
        true,
    ensures
        result == (previous_head < u64::MAX && committed_head == previous_head + 1 && (block_gas_limit == 0 || block_gas_used <= block_gas_limit) && included_count != 0 && included_count == staged_count && safe_included_count == included_count),
{
    previous_head < u64::MAX
        && committed_head == previous_head + 1
        && (block_gas_limit == 0 || block_gas_used <= block_gas_limit)
        && included_count != 0
        && included_count == staged_count
        && safe_included_count == included_count
}
