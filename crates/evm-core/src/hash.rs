//! どこで: Phase1のハッシュ規則 / 何を: tx_id/tx_list_hash/block_hash / なぜ: 決定性を保証するため

use alloy_primitives::keccak256 as alloy_keccak256;
use alloy_primitives::Keccak256;
use evm_db::chain_data::TxKind;
pub use ic_evm_address::derive_evm_address_from_principal;

pub const HASH_LEN: usize = 32;

pub fn keccak256(data: &[u8]) -> [u8; HASH_LEN] {
    alloy_keccak256(data).0
}

pub fn keccak256_concat_chunks(chunks: &[[u8; HASH_LEN]]) -> [u8; HASH_LEN] {
    let mut hasher = Keccak256::new();
    for chunk in chunks.iter() {
        hasher.update(chunk);
    }
    hasher.finalize().0
}

pub fn stored_tx_id(
    kind: TxKind,
    raw: &[u8],
    caller_evm: Option<[u8; 20]>,
    canister_id: Option<&[u8]>,
    caller_principal: Option<&[u8]>,
) -> [u8; HASH_LEN] {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"ic-evm:storedtx:v2");
    buf.push(kind.to_u8());
    buf.extend_from_slice(raw);
    if let Some(caller) = caller_evm {
        buf.extend_from_slice(&caller);
    }
    if let Some(bytes) = canister_id {
        let len = u16::try_from(bytes.len()).unwrap_or(0);
        buf.extend_from_slice(&len.to_be_bytes());
        buf.extend_from_slice(bytes);
    }
    if let Some(bytes) = caller_principal {
        let len = u16::try_from(bytes.len()).unwrap_or(0);
        buf.extend_from_slice(&len.to_be_bytes());
        buf.extend_from_slice(bytes);
    }
    keccak256(&buf)
}

pub fn tx_list_hash(tx_ids: &[[u8; HASH_LEN]]) -> [u8; HASH_LEN] {
    let mut buf = Vec::with_capacity(1 + tx_ids.len() * HASH_LEN);
    buf.push(0x00);
    for tx_id in tx_ids.iter() {
        buf.extend_from_slice(tx_id);
    }
    keccak256(&buf)
}

pub fn block_hash(
    parent_hash: [u8; HASH_LEN],
    number: u64,
    timestamp: u64,
    tx_list_hash: [u8; HASH_LEN],
    state_root: [u8; HASH_LEN],
) -> [u8; HASH_LEN] {
    let mut buf = Vec::with_capacity(1 + HASH_LEN + 8 + 8 + HASH_LEN + HASH_LEN);
    buf.push(0x01);
    buf.extend_from_slice(&parent_hash);
    buf.extend_from_slice(&number.to_be_bytes());
    buf.extend_from_slice(&timestamp.to_be_bytes());
    buf.extend_from_slice(&tx_list_hash);
    buf.extend_from_slice(&state_root);
    keccak256(&buf)
}
