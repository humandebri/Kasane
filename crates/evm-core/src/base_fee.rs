//! どこで: Phase1のbase_fee更新 / 何を: EIP-1559更新式の最小実装 / なぜ: mainnet準備で固定値を避けるため

use evm_db::chain_data::constants::{
    BASE_FEE_MAX_CHANGE_DENOMINATOR, ELASTICITY_MULTIPLIER,
};

pub fn compute_next_base_fee(base_fee: u64, gas_used: u64, block_gas_limit: u64) -> u64 {
    let target_gas = block_gas_limit / ELASTICITY_MULTIPLIER;
    if target_gas == 0 || gas_used == target_gas {
        return base_fee;
    }

    let base_fee_u128 = u128::from(base_fee);
    let target_gas_u128 = u128::from(target_gas);
    let change_denominator = u128::from(BASE_FEE_MAX_CHANGE_DENOMINATOR);
    let delta = if gas_used > target_gas {
        let gas_delta = u128::from(gas_used - target_gas);
        base_fee_u128
            .saturating_mul(gas_delta)
            .checked_div(target_gas_u128)
            .unwrap_or(0)
            .checked_div(change_denominator)
            .unwrap_or(0)
    } else {
        let gas_delta = u128::from(target_gas - gas_used);
        base_fee_u128
            .saturating_mul(gas_delta)
            .checked_div(target_gas_u128)
            .unwrap_or(0)
            .checked_div(change_denominator)
            .unwrap_or(0)
    };

    let next = if gas_used > target_gas {
        base_fee_u128.saturating_add(delta)
    } else {
        base_fee_u128.saturating_sub(delta)
    };

    u64::try_from(next).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::compute_next_base_fee;

    #[test]
    fn base_fee_updates_up_down_and_flat() {
        let base_fee = 100u64;
        let block_gas_limit = 8u64;

        let same = compute_next_base_fee(base_fee, 4, block_gas_limit);
        assert_eq!(same, 100);

        let up = compute_next_base_fee(base_fee, 8, block_gas_limit);
        assert_eq!(up, 112);

        let down = compute_next_base_fee(base_fee, 0, block_gas_limit);
        assert_eq!(down, 88);
    }
}
