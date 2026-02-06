//! どこで: evm-core migration test / 何を: tx_locs key cursor移行 / なぜ: tick再開の正当性を担保するため

use evm_core::chain::migrate_tx_locs_batch;
use evm_db::chain_data::{TxId, TxLoc};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};

fn mk_tx_id(seed: u8) -> TxId {
    let mut buf = [0u8; 32];
    buf[0] = seed;
    buf[31] = seed.wrapping_add(1);
    TxId(buf)
}

#[test]
fn tx_locs_key_cursor_migrates_in_multiple_ticks() {
    init_stable_state();
    let tx_ids = [mk_tx_id(1), mk_tx_id(2), mk_tx_id(3)];

    with_state_mut(|state| {
        for (idx, tx_id) in tx_ids.iter().enumerate() {
            state.tx_locs.insert(*tx_id, TxLoc::queued(idx as u64));
        }
    });

    let (last_key, copied, done) = migrate_tx_locs_batch(None, 2);
    assert_eq!(copied, 2);
    assert!(!done);
    assert!(last_key.is_some());

    let (last_key, copied, done) = migrate_tx_locs_batch(last_key, 2);
    assert_eq!(copied, 1);
    assert!(done);
    assert!(last_key.is_some());

    with_state(|state| {
        assert_eq!(state.tx_locs.len(), state.tx_locs_v3.len());
        for tx_id in tx_ids.iter() {
            let old = state.tx_locs.get(tx_id).unwrap();
            let new = state.tx_locs_v3.get(tx_id).unwrap();
            assert_eq!(old, new);
        }
    });
}
