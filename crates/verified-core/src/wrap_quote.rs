//! どこで: wrap quote 境界 / 何を: 承認上限とfee構成 / なぜ: ユーザー承認を超えるfee徴収を防ぐため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const GAS_PRICE_DENOMINATOR_BPS: u128 = 10_000;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const WEI_PER_E8S: u128 = 10_000_000_000;

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result == (
        ledger_matches == 1
        && charged_fee_e8s <= max_fee_e8s
        && charged_gas_price_wei <= quoted_gas_price_wei
    ),
))]
pub fn wrap_quote_approval_safe_raw(
    ledger_matches: u64,
    charged_fee_e8s: u128,
    max_fee_e8s: u128,
    charged_gas_price_wei: u128,
    quoted_gas_price_wei: u128,
) -> bool {
    ledger_matches == 1
        && charged_fee_e8s <= max_fee_e8s
        && charged_gas_price_wei <= quoted_gas_price_wei
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result == (
        gas_price_component_matches == 1
        && gas_fee_component_matches == 1
        && charged_fee_component_matches == 1
    ),
))]
pub fn wrap_quote_components_safe_raw(
    gas_price_component_matches: u64,
    gas_fee_component_matches: u64,
    charged_fee_component_matches: u64,
) -> bool {
    gas_price_component_matches == 1
        && gas_fee_component_matches == 1
        && charged_fee_component_matches == 1
}
