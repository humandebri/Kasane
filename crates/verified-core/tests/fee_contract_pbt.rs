//! どこで: fee PBT / 何を: EIP-1559境界とsaturating fee計算 / なぜ: fee仕様の取り違えを乱択で検出するため

use proptest::prelude::*;
use verified_core::fee::{
    base_fee_reward, effective_gas_price, l2_fee, min_fee_satisfied, total_fee,
};

proptest! {
    #[test]
    fn pbt_fee_calculations_match_eip1559_bounds_and_saturating_totals(
        max_fee in any::<u128>(),
        max_priority in any::<u128>(),
        base_fee in any::<u64>(),
        gas_price in any::<u128>(),
        gas_priority_fee in proptest::option::of(any::<u128>()),
        min_priority_fee in any::<u64>(),
        min_gas_price in any::<u64>(),
        gas_used in any::<u64>(),
        effective_price in any::<u64>(),
        l1_data_fee in any::<u128>(),
        operator_fee in any::<u128>(),
    ) {
        let expected_effective = if max_priority > max_fee || max_fee < u128::from(base_fee) {
            None
        } else {
            u64::try_from(max_fee.min(u128::from(base_fee).saturating_add(max_priority))).ok()
        };
        prop_assert_eq!(effective_gas_price(max_fee, max_priority, base_fee), expected_effective);

        let expected_min_fee = match gas_priority_fee {
            Some(priority) => {
                priority >= u128::from(min_priority_fee)
                    && gas_price >= u128::from(base_fee)
                    && gas_price >= u128::from(base_fee).saturating_add(u128::from(min_priority_fee))
            }
            None => gas_price >= u128::from(min_gas_price),
        };
        prop_assert_eq!(
            min_fee_satisfied(
                gas_price,
                gas_priority_fee,
                base_fee,
                min_priority_fee,
                min_gas_price,
            ),
            expected_min_fee
        );

        let expected_l2 = u128::from(gas_used) * u128::from(effective_price);
        prop_assert_eq!(l2_fee(gas_used, effective_price), expected_l2);
        prop_assert_eq!(
            base_fee_reward(gas_used, base_fee),
            u128::from(gas_used) * u128::from(base_fee)
        );
        prop_assert_eq!(
            total_fee(gas_used, effective_price, l1_data_fee, operator_fee),
            expected_l2.saturating_add(l1_data_fee).saturating_add(operator_fee)
        );
    }
}
