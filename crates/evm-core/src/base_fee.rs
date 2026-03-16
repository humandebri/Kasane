//! どこで: Phase1のbase_fee更新
//! 何を: EIP-1559更新式をローカル実装で計算
//! なぜ: runtime依存を減らし、alloy-eips をテスト用途へ限定するため

pub fn compute_next_base_fee(base_fee: u64, gas_used: u64, block_gas_limit: u64) -> u64 {
    const ELASTICITY_MULTIPLIER: u64 = 2;
    const BASE_FEE_MAX_CHANGE_DENOMINATOR: u64 = 8;

    let elasticity = ELASTICITY_MULTIPLIER;
    let gas_target = block_gas_limit / elasticity;
    if gas_target == 0 {
        return base_fee;
    }

    if gas_used == gas_target {
        return base_fee;
    }

    let base_fee_u128 = u128::from(base_fee);
    let gas_target_u128 = u128::from(gas_target);

    if gas_used > gas_target {
        // EIP-1559: increase branch
        let gas_delta = u128::from(gas_used - gas_target);
        let change = (base_fee_u128 * gas_delta)
            / gas_target_u128
            / u128::from(BASE_FEE_MAX_CHANGE_DENOMINATOR);
        let min_change = change.max(1);
        let next = base_fee_u128.saturating_add(min_change);
        u64::try_from(next).unwrap_or(u64::MAX)
    } else {
        // EIP-1559: decrease branch
        let gas_delta = u128::from(gas_target - gas_used);
        let change = (base_fee_u128 * gas_delta)
            / gas_target_u128
            / u128::from(BASE_FEE_MAX_CHANGE_DENOMINATOR);
        let next = base_fee_u128.saturating_sub(change);
        u64::try_from(next).unwrap_or(0)
    }
}

#[cfg(test)]
#[path = "base_fee_tests.rs"]
mod tests;
