Generate scenario candidates:
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        committed_head >= previous_head
        && (block_gas_limit == 0 || block_gas_used <= block_gas_limit)
        && included_count != 0
        && included_count == staged_count
        && safe_included_count == included_count
    ),
))]
pub fn block_commit_safe_raw(
    previous_head: u64,
    committed_head: u64,
    included_count: usize,
    staged_count: usize,
    safe_included_count: usize,
    block_gas_used: u64,
    block_gas_limit: u64,
) -> bool
{
    committed_head >= previous_head
        && (block_gas_limit == 0 || block_gas_used <= block_gas_limit)
        && included_count != 0
        && included_count == staged_count
        && safe_included_count == included_count
}
