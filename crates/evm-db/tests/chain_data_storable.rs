//! どこで: Phase1テスト / 何を: Tx/Block/ReceiptのStorable / なぜ: 互換性のため

use evm_db::chain_data::receipt::LogEntry;
use evm_db::chain_data::{
    BlockData, CallerKey, ChainStateV1, Head, QueueMeta, ReceiptLike, TxEnvelope, TxId,
    TxIndexEntry, TxKind, TxLoc,
};
use ic_stable_structures::Storable;

#[test]
fn tx_envelope_roundtrip() {
    let tx_id = TxId([0x11u8; 32]);
    let caller = [0x22u8; 20];
    let envelope = TxEnvelope::new_with_caller(tx_id, TxKind::IcSynthetic, vec![1, 2, 3], caller);
    let bytes = envelope.to_bytes();
    let decoded = TxEnvelope::from_bytes(bytes);
    assert_eq!(envelope, decoded);
}

#[test]
fn tx_envelope_accepts_legacy_format() {
    let tx_id = TxId([0x33u8; 32]);
    let tx_bytes = vec![7u8, 8, 9];
    let mut bytes = Vec::new();
    bytes.push(TxKind::EthSigned.to_u8());
    bytes.extend_from_slice(&tx_id.0);
    let len = u32::try_from(tx_bytes.len()).unwrap_or(0);
    bytes.extend_from_slice(&len.to_be_bytes());
    bytes.extend_from_slice(&tx_bytes);
    let decoded = TxEnvelope::from_bytes(bytes.into());
    assert_eq!(decoded.tx_id, tx_id);
    assert_eq!(decoded.kind, TxKind::EthSigned);
    assert_eq!(decoded.tx_bytes, tx_bytes);
    assert_eq!(decoded.caller_evm, None);
}

#[test]
fn block_roundtrip() {
    let tx_ids = vec![TxId([0x22u8; 32]), TxId([0x33u8; 32])];
    let block = BlockData::new(
        1,
        [0x10u8; 32],
        [0x11u8; 32],
        1,
        tx_ids,
        [0x12u8; 32],
        [0x13u8; 32],
    );
    let bytes = block.to_bytes();
    let decoded = BlockData::from_bytes(bytes);
    assert_eq!(block, decoded);
}

#[test]
fn receipt_roundtrip() {
    let receipt = ReceiptLike {
        tx_id: TxId([0x44u8; 32]),
        block_number: 2,
        tx_index: 0,
        status: 1,
        gas_used: 21000,
        effective_gas_price: 0,
        return_data_hash: [0x55u8; 32],
        return_data: vec![1, 2, 3],
        contract_address: None,
        logs: vec![LogEntry {
            address: [0x11u8; 20],
            topics: vec![[0x22u8; 32]],
            data: vec![0x33, 0x44],
        }],
    };
    let bytes = receipt.to_bytes();
    let decoded = ReceiptLike::from_bytes(bytes);
    assert_eq!(receipt, decoded);
}

#[test]
fn head_roundtrip() {
    let head = Head {
        number: 9,
        block_hash: [0x66u8; 32],
        timestamp: 10,
    };
    let bytes = head.to_bytes();
    let decoded = Head::from_bytes(bytes);
    assert_eq!(head, decoded);
}

#[test]
fn queue_meta_roundtrip() {
    let mut meta = QueueMeta::new();
    meta.push();
    meta.push();
    let bytes = meta.to_bytes();
    let decoded = QueueMeta::from_bytes(bytes);
    assert_eq!(meta, decoded);
}

#[test]
fn tx_index_roundtrip() {
    let entry = TxIndexEntry {
        block_number: 7,
        tx_index: 2,
    };
    let bytes = entry.to_bytes();
    let decoded = TxIndexEntry::from_bytes(bytes);
    assert_eq!(entry, decoded);
}

#[test]
fn tx_loc_roundtrip() {
    let loc = TxLoc::queued(42);
    let bytes = loc.to_bytes();
    let decoded = TxLoc::from_bytes(bytes);
    assert_eq!(loc, decoded);
}

#[test]
fn chain_state_roundtrip() {
    let mut state = ChainStateV1::new(4_801_360);
    state.last_block_number = 10;
    state.last_block_time = 11;
    state.auto_mine_enabled = true;
    state.is_producing = true;
    state.mining_scheduled = false;
    state.next_queue_seq = 12;
    state.mining_interval_ms = 7_000;
    state.base_fee = 1;
    state.min_gas_price = 2;
    state.min_priority_fee = 3;
    let bytes = state.to_bytes();
    let decoded = ChainStateV1::from_bytes(bytes);
    assert_eq!(state, decoded);
}

#[test]
fn caller_key_roundtrip() {
    let key = CallerKey::from_principal_bytes(&[1u8; 5]);
    let bytes = key.to_bytes();
    let decoded = CallerKey::from_bytes(bytes);
    assert_eq!(key, decoded);
}
