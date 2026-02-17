//! どこで: export API のテスト / 何を: cursor整合・max_bytes・Pruned / なぜ: 仕様の逸脱を防ぐため

use alloy_primitives::keccak256;
use evm_core::export::{export_blocks, ExportCursor, ExportError};
use evm_db::chain_data::{BlockData, ReceiptLike, StoredTxBytes, TxId, TxIndexEntry, TxKind, TxLoc};
use evm_db::stable_state::{init_stable_state, with_state_mut};
use evm_db::Storable;
use ic_evm_address::derive_evm_address_from_principal;

#[test]
fn export_cursor_alignment_and_next_cursor() {
    init_stable_state();
    let tx = build_ic_synthetic_tx(0x11, 0);
    let block = make_block(1, tx);

    with_state_mut(|state| {
        state.tx_store.insert(tx, build_ic_synthetic_envelope(0x11, 0));
        insert_block(state, 1, &block);
        insert_receipt(state, tx, 1);
        insert_tx_index(state, tx, 1);
        state.tx_locs.insert(tx, TxLoc::included(1, 0));
        let mut head = *state.head.get();
        head.number = 1;
        state.head.set(head);
    });

    let cursor = ExportCursor {
        block_number: 1,
        segment: 0,
        byte_offset: 1,
    };
    let result = export_blocks(Some(cursor), 10).expect("export should succeed");
    assert!(!result.chunks.is_empty());
    let first = &result.chunks[0];
    assert_eq!(first.segment, 0);
    assert_eq!(first.start, 1);
    let taken = u32::try_from(first.bytes.len()).unwrap_or(0);
    let next = result.next_cursor.expect("next cursor");
    assert_eq!(next.block_number, 1);
    assert_eq!(next.segment, 0);
    assert_eq!(next.byte_offset, first.start.saturating_add(taken));
}

#[test]
fn export_respects_max_bytes() {
    init_stable_state();
    let tx = build_ic_synthetic_tx(0x22, 0);
    let block = make_block(2, tx);
    with_state_mut(|state| {
        state.tx_store.insert(tx, build_ic_synthetic_envelope(0x22, 0));
        insert_block(state, 2, &block);
        insert_receipt(state, tx, 2);
        insert_tx_index(state, tx, 2);
        state.tx_locs.insert(tx, TxLoc::included(2, 0));
        let mut head = *state.head.get();
        head.number = 2;
        state.head.set(head);
    });

    let cursor = ExportCursor {
        block_number: 2,
        segment: 0,
        byte_offset: 0,
    };
    let result = export_blocks(Some(cursor), 1).expect("export should succeed");
    let total: usize = result.chunks.iter().map(|c| c.bytes.len()).sum();
    assert!(total <= 1);
}

#[test]
fn export_pruned_and_oldest_exportable() {
    init_stable_state();
    let tx = build_ic_synthetic_tx(0x33, 0);
    let block = make_block(6, tx);
    with_state_mut(|state| {
        state.tx_store.insert(tx, build_ic_synthetic_envelope(0x33, 0));
        insert_block(state, 6, &block);
        insert_receipt(state, tx, 6);
        insert_tx_index(state, tx, 6);
        state.tx_locs.insert(tx, TxLoc::included(6, 0));
        let mut head = *state.head.get();
        head.number = 6;
        state.head.set(head);

        let mut prune_state = *state.prune_state.get();
        prune_state.set_pruned_before(5);
        state.prune_state.set(prune_state);
    });

    let cursor = ExportCursor {
        block_number: 5,
        segment: 0,
        byte_offset: 0,
    };
    let err = export_blocks(Some(cursor), 10).expect_err("should be pruned");
    assert!(matches!(err, ExportError::Pruned { .. }));

    let ok = export_blocks(None, 10).expect("export should succeed");
    assert!(!ok.chunks.is_empty());
    let next = ok.next_cursor.expect("next cursor");
    assert_eq!(next.block_number, 6);
}

#[test]
fn export_advances_across_blocks_when_budget_allows() {
    init_stable_state();
    let tx1 = build_ic_synthetic_tx(0x44, 0);
    let tx2 = build_ic_synthetic_tx(0x55, 1);
    let block1 = make_block(1, tx1);
    let block2 = make_block(2, tx2);
    with_state_mut(|state| {
        state.tx_store.insert(tx1, build_ic_synthetic_envelope(0x44, 0));
        insert_block(state, 1, &block1);
        insert_receipt(state, tx1, 1);
        insert_tx_index(state, tx1, 1);
        state.tx_locs.insert(tx1, TxLoc::included(1, 0));

        state.tx_store.insert(tx2, build_ic_synthetic_envelope(0x55, 1));
        insert_block(state, 2, &block2);
        insert_receipt(state, tx2, 2);
        insert_tx_index(state, tx2, 2);
        state.tx_locs.insert(tx2, TxLoc::included(2, 0));

        let mut head = *state.head.get();
        head.number = 2;
        state.head.set(head);
    });

    let cursor = ExportCursor {
        block_number: 1,
        segment: 0,
        byte_offset: 0,
    };
    let result = export_blocks(Some(cursor), 1_000_000).expect("export should succeed");
    assert!(!result.chunks.is_empty());
    let next = result.next_cursor.expect("next cursor");
    assert_eq!(next.block_number, 3);
}

#[test]
fn export_rejects_segment_out_of_range() {
    init_stable_state();
    let tx = build_ic_synthetic_tx(0x66, 0);
    let block = make_block(1, tx);
    with_state_mut(|state| {
        state.tx_store.insert(tx, build_ic_synthetic_envelope(0x66, 0));
        insert_block(state, 1, &block);
        insert_receipt(state, tx, 1);
        insert_tx_index(state, tx, 1);
        state.tx_locs.insert(tx, TxLoc::included(1, 0));
        let mut head = *state.head.get();
        head.number = 1;
        state.head.set(head);
    });

    let cursor = ExportCursor {
        block_number: 1,
        segment: 3,
        byte_offset: 0,
    };
    let err = export_blocks(Some(cursor), 10).expect_err("should be invalid cursor");
    assert!(matches!(err, ExportError::InvalidCursor(_)));
}

#[test]
fn export_advances_on_segment_boundary() {
    init_stable_state();
    let tx = build_ic_synthetic_tx(0x77, 0);
    let block = make_block(1, tx);
    with_state_mut(|state| {
        state.tx_store.insert(tx, build_ic_synthetic_envelope(0x77, 0));
        insert_block(state, 1, &block);
        insert_receipt(state, tx, 1);
        insert_tx_index(state, tx, 1);
        state.tx_locs.insert(tx, TxLoc::included(1, 0));
        let mut head = *state.head.get();
        head.number = 1;
        state.head.set(head);
    });

    let block_bytes = block.to_bytes().into_owned();
    let cursor = ExportCursor {
        block_number: 1,
        segment: 0,
        byte_offset: u32::try_from(block_bytes.len()).unwrap_or(0),
    };
    let result = export_blocks(Some(cursor), 1).expect("export should succeed");
    assert!(!result.chunks.is_empty());
    let next = result.next_cursor.expect("next cursor");
    assert_eq!(next.block_number, 1);
    assert_eq!(next.segment, 1);
    assert_eq!(next.byte_offset, 1);
}

#[test]
fn export_tx_index_payload_contains_from_and_to() {
    init_stable_state();
    let tx = build_ic_synthetic_tx(0x88, 9);
    let block = make_block(1, tx);
    with_state_mut(|state| {
        state.tx_store.insert(tx, build_ic_synthetic_envelope(0x88, 9));
        insert_block(state, 1, &block);
        insert_receipt(state, tx, 1);
        insert_tx_index(state, tx, 1);
        state.tx_locs.insert(tx, TxLoc::included(1, 0));
        let mut head = *state.head.get();
        head.number = 1;
        state.head.set(head);
    });
    let result = export_blocks(
        Some(ExportCursor {
            block_number: 1,
            segment: 0,
            byte_offset: 0,
        }),
        1_000_000,
    )
    .expect("export should succeed");
    let segment2 = collect_segment_bytes(&result, 2);
    assert!(!segment2.is_empty());
    let decoded = decode_single_tx_index_entry(&segment2).expect("decode entry");
    assert_eq!(decoded.tx_hash, tx.0.to_vec());
    assert_eq!(decoded.block_number, 1);
    assert_eq!(decoded.tx_index, 0);
    assert_eq!(decoded.from, derive_evm_address_from_principal(&[0x88]).expect("must derive"));
    assert_eq!(decoded.to, Some([0x10; 20]));
}

#[test]
fn export_fails_when_tx_decode_fails() {
    init_stable_state();
    let tx = build_ic_synthetic_tx_with_raw(0x99, vec![0x01, 0x02, 0x03]);
    let block = make_block(1, tx.tx_id);
    with_state_mut(|state| {
        state.tx_store.insert(tx.tx_id, tx.envelope);
        insert_block(state, 1, &block);
        insert_receipt(state, tx.tx_id, 1);
        insert_tx_index(state, tx.tx_id, 1);
        state.tx_locs.insert(tx.tx_id, TxLoc::included(1, 0));
        let mut head = *state.head.get();
        head.number = 1;
        state.head.set(head);
    });
    let err = export_blocks(
        Some(ExportCursor {
            block_number: 1,
            segment: 0,
            byte_offset: 0,
        }),
        1_000_000,
    )
    .expect_err("decode must fail");
    assert!(matches!(err, ExportError::MissingData("tx decode failed")));
}

fn make_block(number: u64, tx_id: TxId) -> BlockData {
    let parent_hash = [0u8; 32];
    let number_u8 = u8::try_from(number).unwrap_or(0);
    let block_hash = [number_u8; 32];
    let tx_list_hash = [number_u8; 32];
    let state_root = [0u8; 32];
    BlockData::new(
        number,
        parent_hash,
        block_hash,
        number,
        1_000_000_000,
        3_000_000,
        21_000,
        vec![tx_id],
        tx_list_hash,
        state_root,
    )
}

fn insert_block(state: &mut evm_db::stable_state::StableState, number: u64, block: &BlockData) {
    let bytes = block.to_bytes().into_owned();
    let ptr = state.blob_store.store_bytes(&bytes).expect("store block");
    state.blocks.insert(number, ptr);
}

fn insert_receipt(state: &mut evm_db::stable_state::StableState, tx_id: TxId, block_number: u64) {
    let receipt = fake_receipt(tx_id, block_number);
    let bytes = receipt.to_bytes().into_owned();
    let ptr = state.blob_store.store_bytes(&bytes).expect("store receipt");
    state.receipts.insert(tx_id, ptr);
}

fn insert_tx_index(state: &mut evm_db::stable_state::StableState, tx_id: TxId, block_number: u64) {
    let entry = TxIndexEntry {
        block_number,
        tx_index: 0,
    };
    let bytes = entry.to_bytes().into_owned();
    let ptr = state
        .blob_store
        .store_bytes(&bytes)
        .expect("store tx_index");
    state.tx_index.insert(tx_id, ptr);
}

fn fake_receipt(tx_id: TxId, block_number: u64) -> ReceiptLike {
    ReceiptLike {
        tx_id,
        block_number,
        tx_index: 0,
        status: 1,
        gas_used: 0,
        effective_gas_price: 0,
        l1_data_fee: 0,
        operator_fee: 0,
        total_fee: 0,
        return_data_hash: [0u8; 32],
        return_data: Vec::new(),
        contract_address: None,
        logs: Vec::new(),
    }
}

struct BuiltEnvelope {
    tx_id: TxId,
    envelope: StoredTxBytes,
}

fn build_ic_synthetic_tx(marker: u8, nonce: u64) -> TxId {
    build_ic_synthetic_tx_with_raw(marker, build_ic_tx_bytes([0x10; 20], nonce)).tx_id
}

fn build_ic_synthetic_envelope(marker: u8, nonce: u64) -> StoredTxBytes {
    build_ic_synthetic_tx_with_raw(marker, build_ic_tx_bytes([0x10; 20], nonce)).envelope
}

fn build_ic_synthetic_tx_with_raw(marker: u8, raw: Vec<u8>) -> BuiltEnvelope {
    let caller_principal = vec![marker];
    let canister_id = vec![0x01];
    let caller_evm = derive_evm_address_from_principal(&caller_principal).expect("must derive");
    let tx_id = compute_ic_synthetic_tx_id(&raw, caller_evm, &canister_id, &caller_principal);
    let envelope = StoredTxBytes::new_with_fees(
        tx_id,
        TxKind::IcSynthetic,
        raw,
        Some(caller_evm),
        canister_id,
        caller_principal,
        1,
        1,
        false,
    );
    BuiltEnvelope { tx_id, envelope }
}

fn compute_ic_synthetic_tx_id(
    raw: &[u8],
    caller_evm: [u8; 20],
    canister_id: &[u8],
    caller_principal: &[u8],
) -> TxId {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"ic-evm:storedtx:v2");
    buf.push(TxKind::IcSynthetic.to_u8());
    buf.extend_from_slice(raw);
    buf.extend_from_slice(&caller_evm);
    let canister_len = u16::try_from(canister_id.len()).unwrap_or(0);
    buf.extend_from_slice(&canister_len.to_be_bytes());
    buf.extend_from_slice(canister_id);
    let principal_len = u16::try_from(caller_principal.len()).unwrap_or(0);
    buf.extend_from_slice(&principal_len.to_be_bytes());
    buf.extend_from_slice(caller_principal);
    TxId(keccak256(buf).0)
}

fn build_ic_tx_bytes(to: [u8; 20], nonce: u64) -> Vec<u8> {
    let value = [0u8; 32];
    let gas_limit = 50_000u64.to_be_bytes();
    let nonce_bytes = nonce.to_be_bytes();
    let max_fee = 2_000_000_000u128.to_be_bytes();
    let max_priority = 1_000_000_000u128.to_be_bytes();
    let data = Vec::new();
    let data_len = u32::try_from(data.len()).unwrap_or(0).to_be_bytes();
    let mut out = Vec::new();
    out.push(2u8);
    out.extend_from_slice(&to);
    out.extend_from_slice(&value);
    out.extend_from_slice(&gas_limit);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&max_fee);
    out.extend_from_slice(&max_priority);
    out.extend_from_slice(&data_len);
    out.extend_from_slice(&data);
    out
}

fn collect_segment_bytes(result: &evm_core::export::ExportResponse, segment: u8) -> Vec<u8> {
    let mut out = Vec::new();
    for chunk in result.chunks.iter().filter(|chunk| chunk.segment == segment) {
        out.extend_from_slice(&chunk.bytes);
    }
    out
}

struct DecodedEntry {
    tx_hash: Vec<u8>,
    block_number: u64,
    tx_index: u32,
    from: [u8; 20],
    to: Option<[u8; 20]>,
}

fn decode_single_tx_index_entry(payload: &[u8]) -> Result<DecodedEntry, &'static str> {
    if payload.len() < 36 {
        return Err("payload too short");
    }
    let tx_hash = payload[0..32].to_vec();
    let len = u32::from_be_bytes(payload[32..36].try_into().map_err(|_| "len")?) as usize;
    if payload.len() != 36 + len {
        return Err("entry_len mismatch");
    }
    let mut offset = 36usize;
    let block_number = u64::from_be_bytes(payload[offset..offset + 8].try_into().map_err(|_| "block")?);
    offset += 8;
    let tx_index = u32::from_be_bytes(payload[offset..offset + 4].try_into().map_err(|_| "index")?);
    offset += 4;
    let principal_len = u16::from_be_bytes(payload[offset..offset + 2].try_into().map_err(|_| "principal_len")?) as usize;
    offset += 2;
    offset += principal_len;
    if payload.len() < offset + 20 + 1 {
        return Err("missing from/to_len");
    }
    let from = <[u8; 20]>::try_from(&payload[offset..offset + 20]).map_err(|_| "from")?;
    offset += 20;
    let to_len = payload[offset];
    offset += 1;
    let to = if to_len == 0 {
        None
    } else if to_len == 20 {
        let addr = <[u8; 20]>::try_from(&payload[offset..offset + 20]).map_err(|_| "to")?;
        offset += 20;
        Some(addr)
    } else {
        return Err("invalid to_len");
    };
    if offset != payload.len() {
        return Err("trailing bytes");
    }
    Ok(DecodedEntry {
        tx_hash,
        block_number,
        tx_index,
        from,
        to,
    })
}
