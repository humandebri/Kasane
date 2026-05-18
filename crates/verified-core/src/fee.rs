//! どこで: submit/execute fee判定 / 何を: gas priceの純粋計算 / なぜ: IC境界とrevm境界から検証対象を分離するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec(effective => ensures
    max_priority > max_fee ==> effective == Option::<u64>::None,
    max_fee < base_fee as u128 ==> effective == Option::<u64>::None,
    matches!(effective, Some(_)) ==> effective.unwrap() <= max_fee,
    matches!(effective, Some(_)) ==> effective.unwrap() >= base_fee,
))]
pub fn effective_gas_price(max_fee: u128, max_priority: u128, base_fee: u64) -> Option<u64> {
    if max_priority > max_fee {
        return None;
    }
    let base_fee = u128::from(base_fee);
    if max_fee < base_fee {
        return None;
    }
    let capped = max_fee.min(base_fee.saturating_add(max_priority));
    u64::try_from(capped).ok()
}

#[cfg_attr(verus_keep_ghost, verus_spec(satisfied => ensures
    matches!(gas_priority_fee, Some(_)) ==> satisfied == (
        gas_priority_fee.unwrap() >= min_priority_fee as u128
        && gas_price >= base_fee as u128
        && gas_price >= (base_fee as u128).saturating_add(min_priority_fee as u128)
    ),
    matches!(gas_priority_fee, None) ==> satisfied == (gas_price >= min_gas_price as u128),
))]
pub fn min_fee_satisfied(
    gas_price: u128,
    gas_priority_fee: Option<u128>,
    base_fee: u64,
    min_priority_fee: u64,
    min_gas_price: u64,
) -> bool {
    match gas_priority_fee {
        Some(priority) => {
            let min_priority_fee = u128::from(min_priority_fee);
            if priority < min_priority_fee {
                return false;
            }
            let base_fee = u128::from(base_fee);
            let base_plus_min = base_fee.saturating_add(min_priority_fee);
            gas_price >= base_fee && gas_price >= base_plus_min
        }
        None => gas_price >= u128::from(min_gas_price),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(fee => ensures
    fee == (gas_used as u128) * (effective_gas_price as u128),
))]
pub fn l2_fee(gas_used: u64, effective_gas_price: u64) -> u128 {
    #[cfg(verus_keep_ghost)]
    proof! {
        assert((gas_used as int) * (effective_gas_price as int) <= u128::MAX as int)
            by(nonlinear_arith);
    }
    u128::from(gas_used) * u128::from(effective_gas_price)
}

#[cfg_attr(verus_keep_ghost, verus_spec(fee => ensures
    fee >= (gas_used as u128) * (effective_gas_price as u128),
    fee >= l1_data_fee || fee == u128::MAX,
    fee >= operator_fee || fee == u128::MAX,
))]
pub fn total_fee(
    gas_used: u64,
    effective_gas_price: u64,
    l1_data_fee: u128,
    operator_fee: u128,
) -> u128 {
    l2_fee(gas_used, effective_gas_price)
        .saturating_add(l1_data_fee)
        .saturating_add(operator_fee)
}

#[cfg_attr(verus_keep_ghost, verus_spec(reward => ensures
    reward == (gas_used as u128) * (base_fee as u128),
))]
pub fn base_fee_reward(gas_used: u64, base_fee: u64) -> u128 {
    l2_fee(gas_used, base_fee)
}

#[cfg(test)]
mod tests {
    use super::{base_fee_reward, effective_gas_price, l2_fee, min_fee_satisfied, total_fee};

    #[test]
    fn effective_gas_price_caps_priority() {
        assert_eq!(effective_gas_price(10, 3, 5), Some(8));
        assert_eq!(effective_gas_price(7, 7, 0), Some(7));
    }

    #[test]
    fn effective_gas_price_rejects_invalid_bounds() {
        assert_eq!(effective_gas_price(10, 11, 0), None);
        assert_eq!(effective_gas_price(9, 0, 10), None);
        assert_eq!(effective_gas_price(u128::MAX, u128::MAX, u64::MAX), None);
    }

    #[test]
    fn min_fee_satisfied_checks_dynamic_and_legacy() {
        assert!(min_fee_satisfied(30, Some(10), 20, 10, 1));
        assert!(!min_fee_satisfied(29, Some(10), 20, 10, 1));
        assert!(!min_fee_satisfied(30, Some(9), 20, 10, 1));
        assert!(min_fee_satisfied(5, None, 20, 10, 5));
        assert!(!min_fee_satisfied(4, None, 20, 10, 5));
    }

    #[test]
    fn fee_accounting_uses_saturating_components() {
        assert_eq!(l2_fee(21_000, 100), 2_100_000);
        assert_eq!(base_fee_reward(21_000, 7), 147_000);
        assert_eq!(total_fee(21_000, 100, 3, 4), 2_100_007);
        assert_eq!(total_fee(u64::MAX, u64::MAX, u128::MAX, 1), u128::MAX);
    }
}
