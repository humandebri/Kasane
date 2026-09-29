//! どこで: ICP query execution gate model / 何を: 実行可能条件 / なぜ: specgen contractを単一targetファイルへ注入するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[allow(dead_code)]
fn main() {}

// specgen:contract icp_query_execution_gate_safe_raw-c8c66378 1a8be935b0cb6f718b9a43789f66c2e5618caea84cddf06dd190d47094f50956
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (calls_before == 0 && mode_allows_external == 1 && value_is_zero == 1 && parsed_input == 1),
))]
pub fn icp_query_execution_gate_safe_raw(
    calls_before: u64,
    mode_allows_external: u64,
    value_is_zero: u64,
    parsed_input: u64,
) -> bool {
    calls_before == 0 && mode_allows_external == 1 && value_is_zero == 1 && parsed_input == 1
}
