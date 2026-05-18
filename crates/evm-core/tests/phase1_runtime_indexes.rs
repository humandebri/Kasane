//! どこで: Phase1.4テスト / 何を: runtimeインデックス整合性 / なぜ: submit性能最適化後の不変条件を守るため

use evm_core::chain::{self, TxIn};
use evm_core::hash;
use evm_db::chain_data::constants::{DROP_CODE_CALLER_MISSING, DROP_CODE_DECODE};
use evm_db::chain_data::{
    CallerKey, ReadyKey, ReadySeqKey, SenderNonceKey, StoredTxBytes, TxId, TxKind, TxLocKind,
};
use evm_db::stable_state::{clear_map, init_stable_state, with_state, with_state_mut};

mod common;

fn relax_fee_floor_for_tests() {
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1;
        chain_state.min_gas_price = 1;
        chain_state.min_priority_fee = 1;
        state.chain_state.set(chain_state);
    });
}

#[test]
fn queue_snapshot_cursor_is_seq_exclusive() {
    init_stable_state();
    relax_fee_floor_for_tests();
    let _ = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x01],
        canister_id: vec![0x11],
        tx: common::build_default_ic_tx_input(0),
    })
    .expect("submit 1");
    let _ = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x02],
        canister_id: vec![0x12],
        tx: common::build_default_ic_tx_input(0),
    })
    .expect("submit 2");
    let _ = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x03],
        canister_id: vec![0x13],
        tx: common::build_default_ic_tx_input(0),
    })
    .expect("submit 3");

    let page1 = chain::get_queue_snapshot(1, None);
    assert_eq!(page1.items.len(), 1);
    let seq1 = page1.items[0].seq;
    assert_eq!(page1.next_cursor, Some(seq1.saturating_add(1)));

    let page2 = chain::get_queue_snapshot(1, page1.next_cursor);
    assert_eq!(page2.items.len(), 1);
    let seq2 = page2.items[0].seq;
    assert!(seq2 > seq1);
    assert_eq!(page2.next_cursor, Some(seq2.saturating_add(1)));

    let page3 = chain::get_queue_snapshot(8, page2.next_cursor);
    assert_eq!(page3.items.len(), 1);
    assert!(page3.items[0].seq > seq2);
    assert_eq!(page3.next_cursor, None);
}

#[test]
fn principal_pending_and_fee_indexes_track_lifecycle() {
    init_stable_state();
    relax_fee_floor_for_tests();
    let principal = vec![0x44];
    common::fund_account(
        hash::derive_evm_address_from_principal(&principal).expect("must derive"),
        1_000_000_000_000_000_000,
    );
    let tx_id = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: principal.clone(),
        canister_id: vec![0x99],
        tx: common::build_default_ic_tx_input(0),
    })
    .expect("submit");
    with_state(|state| {
        let key = CallerKey::from_principal_bytes(&principal);
        assert_eq!(state.principal_pending_count.get(&key), Some(1));
        assert!(state.pending_fee_key_by_tx_id.get(&tx_id).is_some());
        assert_eq!(state.pending_fee_index.len(), 1);
    });

    let outcome = chain::produce_block(1).expect("produce");
    assert_eq!(outcome.block.tx_ids.len(), 1);
    common::assert_block_persist_invariants(outcome.block.number, &outcome.block.tx_ids);
    with_state(|state| {
        let key = CallerKey::from_principal_bytes(&principal);
        assert_eq!(state.principal_pending_count.get(&key), None);
        assert_eq!(state.pending_fee_key_by_tx_id.len(), 0);
        assert_eq!(state.pending_fee_index.len(), 0);
    });
}

#[test]
fn rebuild_runtime_indexes_recovers_from_empty_indexes() {
    init_stable_state();
    relax_fee_floor_for_tests();
    let _ = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x10],
        canister_id: vec![0x20],
        tx: common::build_default_ic_tx_input(0),
    })
    .expect("submit a");
    let _ = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x11],
        canister_id: vec![0x21],
        tx: common::build_default_ic_tx_input(0),
    })
    .expect("submit b");

    with_state_mut(|state| {
        clear_map(&mut state.principal_pending_count);
        clear_map(&mut state.pending_fee_index);
        clear_map(&mut state.pending_fee_key_by_tx_id);
        clear_map(&mut state.ready_by_seq);
    });

    chain::rebuild_pending_runtime_indexes();
    common::assert_runtime_indexes_match_pending();

    with_state(|state| {
        let pending_len = state.pending_by_sender_nonce.len();
        assert_eq!(state.pending_fee_key_by_tx_id.len(), pending_len);
        assert_eq!(state.pending_fee_index.len(), pending_len);
        assert_eq!(state.ready_by_seq.len(), state.ready_key_by_tx_id.len());
        let mut principal_total = 0u64;
        for entry in state.principal_pending_count.iter() {
            principal_total = principal_total.saturating_add(u64::from(entry.value()));
        }
        assert_eq!(principal_total, pending_len);
    });
}

#[test]
fn rebuild_fee_index_keeps_entries_even_when_unaffordable() {
    init_stable_state();
    relax_fee_floor_for_tests();
    let _ = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x50],
        canister_id: vec![0x60],
        tx: common::build_default_ic_tx_input(0),
    })
    .expect("submit");
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = u64::MAX;
        state.chain_state.set(chain_state);
    });

    chain::rebuild_pending_runtime_indexes();
    common::assert_runtime_indexes_match_pending();

    with_state(|state| {
        let pending_len = state.pending_by_sender_nonce.len();
        assert_eq!(pending_len, 1);
        assert_eq!(state.pending_fee_key_by_tx_id.len(), pending_len);
        assert_eq!(state.pending_fee_index.len(), pending_len);
    });
}

#[test]
fn produce_block_outcome_reports_dropped_count() {
    init_stable_state();
    relax_fee_floor_for_tests();
    let good_principal = vec![0x20];
    common::fund_account(
        hash::derive_evm_address_from_principal(&good_principal).expect("must derive"),
        1_000_000_000_000_000_000,
    );
    let good_id = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: good_principal,
        canister_id: vec![0x30],
        tx: common::build_default_ic_tx_input(0),
    })
    .expect("submit good");
    let bad_id = TxId([0xabu8; 32]);
    with_state_mut(|state| {
        // caller_evm が None の IcSynthetic は decode 時に drop される。
        state.tx_store.insert(
            bad_id,
            StoredTxBytes::new_with_fees(
                bad_id,
                TxKind::IcSynthetic,
                vec![0u8; 16],
                None,
                vec![0x30],
                vec![0x20],
                2_000_000_000,
                1_000_000_000,
                true,
            ),
        );
        let bad_ready = ReadyKey::new(2_000_000_000, 1_000_000_000, u64::MAX - 1, bad_id.0);
        state.ready_queue.insert(bad_ready, bad_id);
        state.ready_key_by_tx_id.insert(bad_id, bad_ready);
        state
            .ready_by_seq
            .insert(ReadySeqKey::new(u64::MAX - 1, bad_id.0), bad_id);
    });

    let outcome = chain::produce_block(2).expect("produce");
    assert_eq!(outcome.block.tx_ids, vec![good_id]);
    assert_eq!(outcome.dropped, 1);
    common::assert_block_persist_invariants(outcome.block.number, &outcome.block.tx_ids);
    common::assert_dropped_tx_purged(bad_id, DROP_CODE_CALLER_MISSING);
}

#[test]
fn rebuild_runtime_indexes_drops_decode_broken_pending_entries() {
    init_stable_state();
    relax_fee_floor_for_tests();
    let good_id = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x70],
        canister_id: vec![0x80],
        tx: common::build_default_ic_tx_input(0),
    })
    .expect("submit good");
    let bad_id = TxId([0xceu8; 32]);
    let bad_sender = [0x66u8; 20];
    let bad_pending = SenderNonceKey::new(bad_sender, 0);
    let decode_before = with_state(|state| {
        let idx = usize::from(DROP_CODE_DECODE);
        state.metrics_state.get().drop_counts[idx]
    });
    with_state_mut(|state| {
        state.tx_store.insert(
            bad_id,
            StoredTxBytes::new_with_fees(
                bad_id,
                TxKind::EthSigned,
                Vec::new(),
                None,
                Vec::new(),
                vec![0x66],
                1,
                0,
                false,
            ),
        );
        state
            .tx_locs
            .insert(bad_id, evm_db::chain_data::TxLoc::queued(10));
        state.pending_by_sender_nonce.insert(bad_pending, bad_id);
        state.pending_meta_by_tx_id.insert(bad_id, bad_pending);
        let bad_ready = ReadyKey::new(1, 0, 10, bad_id.0);
        state.ready_queue.insert(bad_ready, bad_id);
        state.ready_key_by_tx_id.insert(bad_id, bad_ready);
        state
            .ready_by_seq
            .insert(ReadySeqKey::new(10, bad_id.0), bad_id);
    });

    chain::rebuild_pending_runtime_indexes();
    common::assert_runtime_indexes_match_pending();

    with_state(|state| {
        assert!(state.tx_store.get(&bad_id).is_none());
        assert!(state.pending_by_sender_nonce.get(&bad_pending).is_none());
        assert!(state.pending_meta_by_tx_id.get(&bad_id).is_none());
        assert!(state.ready_key_by_tx_id.get(&bad_id).is_none());
        assert!(state
            .ready_by_seq
            .get(&ReadySeqKey::new(10, bad_id.0))
            .is_none());
        let loc = chain::get_tx_loc(&bad_id).expect("bad tx loc");
        assert_eq!(loc.kind, TxLocKind::Dropped);
        assert_eq!(loc.drop_code, DROP_CODE_DECODE);
        assert!(state.tx_store.get(&good_id).is_some());
        let idx = usize::from(DROP_CODE_DECODE);
        assert_eq!(
            state.metrics_state.get().drop_counts[idx],
            decode_before.saturating_add(1)
        );
    });
}

#[test]
fn rebuild_eth_tx_hash_index_matches_tx_store() {
    init_stable_state();
    let raw_a = vec![0x01, 0x02, 0x03];
    let raw_b = vec![0x04, 0x05, 0x06];
    let tx_a = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw_a,
        None,
        None,
        None,
    ));
    let tx_b = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw_b,
        None,
        None,
        None,
    ));
    with_state_mut(|state| {
        state.tx_store.insert(
            tx_a,
            StoredTxBytes::new_with_fees(
                tx_a,
                TxKind::EthSigned,
                raw_a,
                None,
                Vec::new(),
                Vec::new(),
                1,
                0,
                false,
            ),
        );
        state.tx_store.insert(
            tx_b,
            StoredTxBytes::new_with_fees(
                tx_b,
                TxKind::EthSigned,
                raw_b,
                None,
                Vec::new(),
                Vec::new(),
                1,
                0,
                false,
            ),
        );
        clear_map(&mut state.eth_tx_hash_index);
    });

    let (cursor, rebuilt, done) = chain::rebuild_eth_tx_hash_index_batch(None, 1);
    assert_eq!(rebuilt, 1);
    assert!(!done);
    let (_, rebuilt, done) = chain::rebuild_eth_tx_hash_index_batch(cursor, 4);
    assert_eq!(rebuilt, 1);
    assert!(done);

    let (ok, indexed, expected) = chain::verify_eth_tx_hash_index(16);
    assert!(ok, "indexed={indexed} expected={expected}");
    assert_eq!(indexed, 2);
    assert_eq!(expected, 2);
}
