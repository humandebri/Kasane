//! どこで: Phase1のstate_root計算 / 何を: 全件走査の決定的root / なぜ: 正しさ優先のため

use crate::hash::keccak256;
use evm_db::stable_state::StableState;
#[cfg(feature = "full_state_root")]
use evm_db::stable_state::with_state;
use ic_stable_structures::Storable;

fn leaf_hash(key_bytes: &[u8], value_bytes: &[u8]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(key_bytes.len() + value_bytes.len());
    buf.extend_from_slice(key_bytes);
    buf.extend_from_slice(value_bytes);
    keccak256(&buf)
}

#[allow(dead_code)]
pub fn compute_full_state_root_with(state: &StableState) -> [u8; 32] {
    let mut acc = Vec::new();
    for entry in state.accounts.iter() {
        let key = entry.key().to_bytes().into_owned();
        let value = entry.value().to_bytes().into_owned();
        let leaf = leaf_hash(&key, &value);
        acc.extend_from_slice(&leaf);
    }
    for entry in state.storage.iter() {
        let key = entry.key().to_bytes().into_owned();
        let value = entry.value().to_bytes().into_owned();
        let leaf = leaf_hash(&key, &value);
        acc.extend_from_slice(&leaf);
    }
    for entry in state.codes.iter() {
        let key = entry.key().to_bytes().into_owned();
        let value = entry.value().to_bytes().into_owned();
        let leaf = leaf_hash(&key, &value);
        acc.extend_from_slice(&leaf);
    }
    keccak256(&acc)
}

#[cfg(feature = "full_state_root")]
pub fn compute_full_state_root() -> [u8; 32] {
    with_state(compute_full_state_root_with)
}

pub fn compute_block_change_hash(tx_change_hashes: &[[u8; 32]]) -> [u8; 32] {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"ic-evm:block-changes:v1");
    for hash in tx_change_hashes.iter() {
        buf.extend_from_slice(hash);
    }
    keccak256(&buf)
}

pub fn compute_state_root_from_changes(
    prev_state_root: [u8; 32],
    block_number: u64,
    tx_list_hash: [u8; 32],
    block_change_hash: [u8; 32],
) -> [u8; 32] {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"ic-evm:state-root:v1");
    buf.extend_from_slice(&prev_state_root);
    buf.extend_from_slice(&block_number.to_be_bytes());
    buf.extend_from_slice(&tx_list_hash);
    buf.extend_from_slice(&block_change_hash);
    keccak256(&buf)
}

pub fn empty_tx_change_hash() -> [u8; 32] {
    keccak256(b"ic-evm:tx-change:v1")
}
