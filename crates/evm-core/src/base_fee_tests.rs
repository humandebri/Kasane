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
