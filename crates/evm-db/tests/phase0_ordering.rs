//! どこで: Phase0テスト / 何を: キー辞書順とOverlay順序 / なぜ: 決定性の担保

use evm_db::overlay::OverlayMap;
use evm_db::types::keys::{make_account_key, make_storage_key};

#[test]
fn key_prefix_ordering_is_stable() {
    let addr = [0x10u8; 20];
    let slot = [0x20u8; 32];

    let account_key = make_account_key(addr);
    let storage_key = make_storage_key(addr, slot);

    assert!(account_key.0[..] < storage_key.0[..]);
}

#[test]
fn overlay_commit_is_sorted() {
    let mut overlay = OverlayMap::new();
    overlay.set(3u8, "c");
    overlay.set(1u8, "a");
    overlay.set(2u8, "b");

    let mut order = Vec::new();
    overlay.commit_to(|key, _| order.push(*key));

    assert_eq!(order, vec![1u8, 2u8, 3u8]);
}

#[test]
fn overlay_commit_keeps_tombstone() {
    let mut overlay = OverlayMap::new();
    overlay.set(1u8, "a");
    overlay.delete(2u8);

    let mut applied = Vec::new();
    overlay.commit_to(|key, value| applied.push((*key, value.is_some())));

    assert_eq!(applied, vec![(1u8, true), (2u8, false)]);
}

#[test]
fn overlay_drain_is_sorted_and_clears_entries() {
    let mut overlay = OverlayMap::new();
    overlay.set(3u8, "c");
    overlay.set(1u8, "a");
    overlay.delete(2u8);

    let mut applied = Vec::new();
    overlay.drain_to(|key, value| applied.push((key, value.is_some())));

    assert_eq!(applied, vec![(1u8, true), (2u8, false), (3u8, true)]);

    let mut after = Vec::new();
    overlay.commit_to(|key, value| after.push((*key, value.is_some())));
    assert!(after.is_empty());
}
