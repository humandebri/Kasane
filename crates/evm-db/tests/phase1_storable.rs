//! どこで: Phase1テスト / 何を: Tx/Block/ReceiptのStorable / なぜ: 互換性のため

use evm_backend::phase1::{
    BlockData, Head, QueueMeta, ReceiptLike, TxEnvelope, TxId, TxIndexEntry, TxKind,
};
use ic_stable_structures::Storable;

#[test]
fn tx_envelope_roundtrip() {
    let tx_id = TxId([0x11u8; 32]);
    let envelope = TxEnvelope::new(tx_id, TxKind::EthSigned, vec![1, 2, 3]);
    let bytes = envelope.to_bytes();
    let decoded = TxEnvelope::from_bytes(bytes);
    assert_eq!(envelope, decoded);
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
        return_data_hash: [0x55u8; 32],
        contract_address: None,
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
