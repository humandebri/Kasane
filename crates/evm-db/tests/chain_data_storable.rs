//! どこで: Phase1テスト / 何を: Tx/Block/ReceiptのStorable / なぜ: 互換性のため

use evm_db::chain_data::receipt::LogEntry;
use evm_db::chain_data::{
    BlockData, CallerKey, ChainStateV1, Head, OpsMetricsV1, QueueMeta, ReceiptLike, StoredTx,
    StoredTxBytes, TxId, TxIndexEntry, TxKind, TxLoc,
};
use ic_stable_structures::Storable;

#[test]
fn tx_envelope_roundtrip() {
    let tx_id = TxId([0x11u8; 32]);
    let envelope = StoredTxBytes::new_with_fees(
        tx_id,
        TxKind::IcSynthetic,
        vec![1, 2, 3],
        Some([0x22u8; 20]),
        vec![0x01],
        vec![0x02],
        2_000_000_000u128,
        1_000_000_000u128,
        true,
    );
    let bytes = envelope.to_bytes();
    let decoded = StoredTxBytes::from_bytes(bytes);
    assert_eq!(envelope, decoded);
}

#[test]
fn tx_envelope_rejects_unsupported_version_without_trap() {
    let mut bytes = Vec::new();
    bytes.push(1u8);
    bytes.extend_from_slice(&[0u8; 10]);
    let decoded = StoredTxBytes::from_bytes(bytes.into());
    assert!(decoded.is_invalid());
    assert!(decoded.validate().is_err());
    assert_eq!(decoded.kind(), TxKind::EthSigned);
    assert_ne!(decoded.tx_id().0, [0u8; 32]);
}

#[test]
fn stored_tx_rejects_tx_id_mismatch() {
    let tx_id = TxId([0x33u8; 32]);
    let bytes = StoredTxBytes::new_with_fees(
        tx_id,
        TxKind::EthSigned,
        vec![0x01],
        None,
        Vec::new(),
        Vec::new(),
        1,
        0,
        false,
    );
    let result = StoredTx::try_from(bytes);
    assert!(result.is_err());
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
        l1_data_fee: 11,
        operator_fee: 22,
        total_fee: 33,
        return_data_hash: [0x55u8; 32],
        return_data: vec![1, 2, 3],
        contract_address: None,
        logs: vec![test_log(
            [0x11u8; 20],
            vec![[0x22u8; 32]],
            vec![0x33, 0x44],
        )],
    };
    let bytes = receipt.to_bytes();
    let decoded = ReceiptLike::from_bytes(bytes);
    assert_eq!(receipt, decoded);
}

#[test]
fn receipt_log_binary_stability_roundtrip() {
    let receipt = ReceiptLike {
        tx_id: TxId([0x88u8; 32]),
        block_number: 3,
        tx_index: 1,
        status: 1,
        gas_used: 30_000,
        effective_gas_price: 50,
        l1_data_fee: 1,
        operator_fee: 2,
        total_fee: 3,
        return_data_hash: [0x77u8; 32],
        return_data: vec![0xaa, 0xbb],
        contract_address: Some([0x66u8; 20]),
        logs: vec![test_log(
            [0x11u8; 20],
            vec![[0x01u8; 32], [0x02u8; 32]],
            vec![0x00, 0xff, 0x42],
        )],
    };
    let encoded = receipt.to_bytes().into_owned();
    let decoded = ReceiptLike::from_bytes(encoded.clone().into());
    let reencoded = decoded.to_bytes().into_owned();
    assert_eq!(encoded, reencoded);
}

#[test]
fn receipt_decode_rejects_topics_over_limit() {
    let tx_id = TxId([0x99u8; 32]);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"rcptv2\0\x02");
    bytes.extend_from_slice(&tx_id.0);
    bytes.extend_from_slice(&1u64.to_be_bytes()); // block
    bytes.extend_from_slice(&0u32.to_be_bytes()); // index
    bytes.push(1u8); // status
    bytes.extend_from_slice(&21_000u64.to_be_bytes()); // gas used
    bytes.extend_from_slice(&1u64.to_be_bytes()); // effective gas price
    bytes.extend_from_slice(&0u128.to_be_bytes()); // l1_data_fee
    bytes.extend_from_slice(&0u128.to_be_bytes()); // operator_fee
    bytes.extend_from_slice(&0u128.to_be_bytes()); // total_fee
    bytes.extend_from_slice(&[0u8; 32]); // return_data_hash
    bytes.extend_from_slice(&0u32.to_be_bytes()); // return_data len
    bytes.push(0u8); // no contract
    bytes.extend_from_slice(&[0u8; 20]); // contract bytes
    bytes.extend_from_slice(&1u32.to_be_bytes()); // logs len
    bytes.extend_from_slice(&[0x11u8; 20]); // log address
    bytes.extend_from_slice(&5u32.to_be_bytes()); // topics len (invalid)
    for _ in 0..5 {
        bytes.extend_from_slice(&[0x22u8; 32]);
    }
    bytes.extend_from_slice(&0u32.to_be_bytes()); // data len

    let decoded = ReceiptLike::from_bytes(bytes.into());
    assert_eq!(decoded.tx_id, TxId([0u8; 32]));
    assert!(decoded.logs.is_empty());
}

#[test]
fn ops_metrics_roundtrip() {
    let metrics = OpsMetricsV1 {
        schema_version: 1,
        exec_halt_unknown_count: 7,
        last_exec_halt_unknown_warn_ts: 99,
    };
    let decoded = OpsMetricsV1::from_bytes(metrics.to_bytes());
    assert_eq!(metrics, decoded);
}

#[test]
fn ops_metrics_decode_legacy_v1_size() {
    let mut legacy = vec![0u8; 40];
    legacy[0] = 1;
    legacy[8..16].copy_from_slice(&7u64.to_be_bytes());
    legacy[16..24].copy_from_slice(&99u64.to_be_bytes());
    let decoded = OpsMetricsV1::from_bytes(legacy.into());
    assert_eq!(decoded.schema_version, 1);
    assert_eq!(decoded.exec_halt_unknown_count, 7);
    assert_eq!(decoded.last_exec_halt_unknown_warn_ts, 99);
}

#[test]
fn receipt_v1_decode_backfills_new_fee_fields_with_zero() {
    let tx_id = TxId([0x77u8; 32]);
    let mut old = Vec::new();
    old.extend_from_slice(&tx_id.0);
    old.extend_from_slice(&2u64.to_be_bytes());
    old.extend_from_slice(&0u32.to_be_bytes());
    old.push(1u8);
    old.extend_from_slice(&21_000u64.to_be_bytes());
    old.extend_from_slice(&30u64.to_be_bytes());
    old.extend_from_slice(&[0x55u8; 32]);
    old.extend_from_slice(&0u32.to_be_bytes());
    old.push(0u8);
    old.extend_from_slice(&[0u8; 20]);
    old.extend_from_slice(&0u32.to_be_bytes());
    let decoded = ReceiptLike::from_bytes(old.into());
    assert_eq!(decoded.tx_id, tx_id);
    assert_eq!(decoded.l1_data_fee, 0);
    assert_eq!(decoded.operator_fee, 0);
    assert_eq!(decoded.total_fee, 0);
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

fn test_log(address: [u8; 20], topics: Vec<[u8; 32]>, data: Vec<u8>) -> LogEntry {
    let topics = topics
        .into_iter()
        .map(alloy_primitives::B256::from)
        .collect::<Vec<_>>();
    let data = alloy_primitives::Bytes::from(data);
    LogEntry::new_unchecked(alloy_primitives::Address::from(address), topics, data)
}
