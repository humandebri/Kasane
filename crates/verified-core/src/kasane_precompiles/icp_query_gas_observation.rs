//! どこで: ICP query gas model / 何を: outcome別gas条件 / なぜ: specgen contractを単一targetファイルへ注入するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_QUERY_PRECOMPILE_ADDRESS_CODE: u64 = 3;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_QUERY_BASE_GAS: u64 = 50_000;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_QUERY_INPUT_BYTE_GAS: u64 = 16;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_QUERY_REPLY_BYTE_GAS: u64 = 8;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_QUERY_OUTCOME_RETURN: u64 = 1;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_QUERY_OUTCOME_OOG: u64 = 2;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const ICP_QUERY_OUTCOME_OTHER_FAILURE: u64 = 3;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS: u64 = 768_614_336_404_562_567;

#[allow(dead_code)]
fn main() {}

// specgen:contract icp_query_gas_observation_safe_raw-9b7ab62f ddd9750c805680c29597a89219edf91352046ae996bba67d68a9b5f9062bfc1e
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (observed_address_code == ICP_QUERY_PRECOMPILE_ADDRESS_CODE && (outcome == ICP_QUERY_OUTCOME_RETURN || outcome == ICP_QUERY_OUTCOME_OOG || outcome == ICP_QUERY_OUTCOME_OTHER_FAILURE) && (input_len > MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS || reply_len > MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS || charged_gas >= ICP_QUERY_BASE_GAS + input_len * ICP_QUERY_INPUT_BYTE_GAS + reply_len * ICP_QUERY_REPLY_BYTE_GAS) && (outcome != ICP_QUERY_OUTCOME_RETURN || gas_limit >= charged_gas) && (outcome != ICP_QUERY_OUTCOME_OOG || gas_limit < charged_gas)),
))]
pub fn icp_query_gas_observation_safe_raw(
    observed_address_code: u64,
    input_len: u64,
    reply_len: u64,
    charged_gas: u64,
    gas_limit: u64,
    outcome: u64,
) -> bool
{
    let exact_combined_len = input_len <= MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS
        && reply_len <= MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS;
    let exact_charged_gas = if exact_combined_len {
        let input_gas = input_len * ICP_QUERY_INPUT_BYTE_GAS;
        let reply_gas = reply_len * ICP_QUERY_REPLY_BYTE_GAS;
        charged_gas >= ICP_QUERY_BASE_GAS + input_gas + reply_gas
    } else {
        true
    };
    observed_address_code == ICP_QUERY_PRECOMPILE_ADDRESS_CODE
        && (outcome == ICP_QUERY_OUTCOME_RETURN
            || outcome == ICP_QUERY_OUTCOME_OOG
            || outcome == ICP_QUERY_OUTCOME_OTHER_FAILURE)
        && exact_charged_gas
        && (outcome != ICP_QUERY_OUTCOME_RETURN || gas_limit >= charged_gas)
        && (outcome != ICP_QUERY_OUTCOME_OOG || gas_limit < charged_gas)
}
