//! どこで: Phase1のハッシュ規則 / 何を: tx_id/tx_list_hash/block_hash / なぜ: 決定性を保証するため

use tiny_keccak::{Hasher, Keccak};

pub const HASH_LEN: usize = 32;

pub fn keccak256(data: &[u8]) -> [u8; HASH_LEN] {
    let mut out = [0u8; HASH_LEN];
    let mut hasher = Keccak::v256();
    hasher.update(data);
    hasher.finalize(&mut out);
    out
}

pub fn tx_id(tx_bytes: &[u8]) -> [u8; HASH_LEN] {
    keccak256(tx_bytes)
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
