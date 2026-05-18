Generate a concise spec draft candidate:
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        facts.committed_head >= facts.previous_head
        && (facts.block_gas_limit == 0 || facts.block_gas_used <= facts.block_gas_limit)
        &&
        facts.included_count != 0
        && facts.included_count == facts.staged_count
        && facts.safe_included_count == facts.included_count
    ),
))]
pub fn block_commit_safe(facts: BlockCommitFacts) -> bool
{
    facts.committed_head >= facts.previous_head
        && (facts.block_gas_limit == 0 || facts.block_gas_used <= facts.block_gas_limit)
        && facts.included_count != 0
        && facts.included_count == facts.staged_count
        && facts.safe_included_count == facts.included_count
}
