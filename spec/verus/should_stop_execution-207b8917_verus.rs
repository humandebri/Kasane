// Generated Verus contract draft. Do not edit by hand.
// git_commit: 703d7df1dcdc48c6f15be3733c6da4ec5c6a8dad
// worktree_dirty: false
// source_hash: 207b8917aba05a0e57fdbfa90be39c0e28e586fee3db5a9e1ba3ca68e4f6408e
// semantic_hash: 3b26bd46cb29deee1b1b0351d0b169ada91df7214b1a471dafe653286e306225
// verified_subject: harness_only
use vstd::prelude::*;
verus! {
    proof fn accepted_spec_harness()
        // ensures stop == ((block_gas_limit > 0 && block_gas_used >= block_gas_limit) || (instruction_soft_limit > 0 && consumed >= instruction_soft_limit))
        // ensures instruction_current >= instruction_start ==> consumed == instruction_current - instruction_start
        // ensures instruction_current < instruction_start ==> consumed == 0
        // panic_behavior documented
        // overflow_behavior documented
    {}
}

fn main() {}
