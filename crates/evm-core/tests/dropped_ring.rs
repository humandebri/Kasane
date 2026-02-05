//! どこで: dropped管理テスト / 何を: 固定長リングの上限維持 / なぜ: tx_locsメモリリークを防ぐため

use evm_core::chain;
use evm_db::chain_data::constants::DROPPED_RING_CAPACITY;
use evm_db::chain_data::TxLocKind;
use evm_db::stable_state::{init_stable_state, with_state};

#[test]
fn dropped_ring_keeps_tx_locs_bounded() {
    init_stable_state();
    let caller_principal = vec![0x11];
    let canister_id = vec![0x22];
    let mut submitted = Vec::new();

    for i in 0..(DROPPED_RING_CAPACITY + 5) {
        let max_fee = 2_000_000_000u128 + (u128::from(i) * 1_000_000_000u128);
        let tx = build_ic_tx_bytes_with_fee(max_fee, max_fee, 0);
        let tx_id = chain::submit_ic_tx(caller_principal.clone(), canister_id.clone(), tx)
            .unwrap_or_else(|_| panic!("submit failed at {i}"));
        submitted.push(tx_id);
    }

    with_state(|state| {
        let dropped_count = state
            .tx_locs
            .iter()
            .filter(|entry| entry.value().kind == TxLocKind::Dropped)
            .count();
        assert!(dropped_count <= usize::try_from(DROPPED_RING_CAPACITY).unwrap_or(usize::MAX));
    });

    let oldest = submitted[0];
    let newest = submitted[submitted.len() - 2];
    let oldest_loc = chain::get_tx_loc(&oldest);
    assert!(
        oldest_loc.is_none(),
        "oldest dropped entry should be evicted"
    );
    let newest_loc = chain::get_tx_loc(&newest).expect("recent dropped entry must exist");
    assert_eq!(newest_loc.kind, TxLocKind::Dropped);
}

fn build_ic_tx_bytes_with_fee(max_fee: u128, max_priority: u128, nonce: u64) -> Vec<u8> {
    let to = [0u8; 20];
    let value = [0u8; 32];
    let gas_limit = 50_000u64.to_be_bytes();
    let nonce = nonce.to_be_bytes();
    let max_fee = max_fee.to_be_bytes();
    let max_priority = max_priority.to_be_bytes();
    let data: Vec<u8> = Vec::new();
    let data_len = u32::try_from(data.len()).unwrap_or(0).to_be_bytes();
    let mut out = Vec::new();
    out.push(2u8);
    out.extend_from_slice(&to);
    out.extend_from_slice(&value);
    out.extend_from_slice(&gas_limit);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&max_fee);
    out.extend_from_slice(&max_priority);
    out.extend_from_slice(&data_len);
    out.extend_from_slice(&data);
    out
}

#[test]
fn dropped_ring_does_not_remove_included_or_queued() {
    init_stable_state();
    let caller_principal = vec![0x31];
    let canister_id = vec![0x41];
    let included_tx = chain::submit_ic_tx(
        caller_principal.clone(),
        canister_id.clone(),
        build_ic_tx_bytes_with_fee(3_000_000_000, 3_000_000_000, 0),
    )
    .expect("submit included");
    let _ = chain::produce_block(1).expect("produce block");
    assert_eq!(
        chain::get_tx_loc(&included_tx).map(|value| value.kind),
        Some(TxLocKind::Included)
    );

    for i in 0..(DROPPED_RING_CAPACITY + 2) {
        let tx = build_ic_tx_bytes_with_fee(
            4_000_000_000u128 + (u128::from(i) * 1_000_000_000u128),
            4_000_000_000u128 + (u128::from(i) * 1_000_000_000u128),
            1,
        );
        let _ = chain::submit_ic_tx(caller_principal.clone(), canister_id.clone(), tx);
    }

    let after = chain::get_tx_loc(&included_tx).expect("included must remain");
    assert_eq!(after.kind, TxLocKind::Included);
}
