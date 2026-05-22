// Generated Verus contract draft. Do not edit by hand.
// git_commit: 703d7df1dcdc48c6f15be3733c6da4ec5c6a8dad
// worktree_dirty: false
// source_hash: 3dada50dbfbec3c3991a06ad15fca6be5687c8c58a8a3a825fb8ed419d5afc2b
// semantic_hash: fa9487ba51d96176707f328ef5a7921718815d559e995dab958c1269eb12930b
// verified_subject: harness_only
use vstd::prelude::*;
verus! {
    proof fn accepted_spec_harness()
        // ensures incoming_nonce < expected_nonce ==> result == NonceDecision::TooLow
        // ensures incoming_nonce > expected_nonce ==> result == NonceDecision::Gap
        // ensures incoming_nonce == expected_nonce && pending_effective_gas_price == None ==> result == NonceDecision::Accept
        // ensures incoming_nonce == expected_nonce && pending_effective_gas_price == Some(old) && incoming_effective_gas_price <= old ==> result == NonceDecision::Conflict
        // ensures incoming_nonce == expected_nonce && pending_effective_gas_price == Some(old) && incoming_effective_gas_price > old ==> result == NonceDecision::Replace
        // panic_behavior documented
        // overflow_behavior documented
    {}
}

fn main() {}
