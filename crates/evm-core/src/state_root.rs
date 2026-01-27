//! どこで: Phase1のstate_root計算 / 何を: 全件走査の決定的root / なぜ: 正しさ優先のため

use crate::hash::keccak256;
use evm_db::stable_state::with_state;
use ic_stable_structures::Storable;

fn leaf_hash(key_bytes: &[u8], value_bytes: &[u8]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(key_bytes.len() + value_bytes.len());
    buf.extend_from_slice(key_bytes);
    buf.extend_from_slice(value_bytes);
    keccak256(&buf)
}

pub fn compute_state_root() -> [u8; 32] {
    with_state(|state| {
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
    })
}
