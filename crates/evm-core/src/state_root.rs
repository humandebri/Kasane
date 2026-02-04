//! どこで: Phase1のstate_root計算 / 何を: 標準trieでのstate root算出 / なぜ: 参照実装との乖離を減らすため

use crate::hash::keccak256;
use alloy_primitives::{Address, B256, U256};
use alloy_trie::root::{state_root_unhashed, storage_root_unhashed};
use alloy_trie::{TrieAccount, EMPTY_ROOT_HASH, KECCAK_EMPTY};
#[cfg(feature = "full_state_root")]
use evm_db::stable_state::with_state;
use evm_db::stable_state::StableState;
use std::collections::BTreeMap;

pub fn compute_state_root_with(state: &StableState) -> [u8; 32] {
    let mut storage_by_addr: BTreeMap<[u8; 20], Vec<(B256, U256)>> = BTreeMap::new();
    for entry in state.storage.iter() {
        let key = entry.key().0;
        if key[0] != 0x02 {
            continue;
        }
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&key[1..21]);
        let mut slot = [0u8; 32];
        slot.copy_from_slice(&key[21..53]);
        storage_by_addr
            .entry(addr)
            .or_default()
            .push((B256::from(slot), U256::from_be_bytes(entry.value().0)));
    }

    let mut trie_accounts: BTreeMap<[u8; 20], TrieAccount> = BTreeMap::new();
    for entry in state.accounts.iter() {
        let key = entry.key().0;
        if key[0] != 0x01 {
            continue;
        }
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&key[1..21]);
        let account = entry.value();
        let storage_root = storage_by_addr
            .get(&addr)
            .map(|slots| storage_root_unhashed(slots.iter().copied()))
            .unwrap_or(EMPTY_ROOT_HASH);
        let code_hash = normalize_code_hash(B256::from(account.code_hash()));
        let trie_account = TrieAccount {
            nonce: account.nonce(),
            balance: U256::from_be_bytes(account.balance()),
            storage_root,
            code_hash,
        };
        if !is_empty_trie_account(&trie_account) {
            trie_accounts.insert(addr, trie_account);
        }
    }

    for (addr, slots) in storage_by_addr {
        if trie_accounts.contains_key(&addr) {
            continue;
        }
        let trie_account = TrieAccount {
            nonce: 0,
            balance: U256::ZERO,
            storage_root: storage_root_unhashed(slots),
            code_hash: KECCAK_EMPTY,
        };
        if !is_empty_trie_account(&trie_account) {
            trie_accounts.insert(addr, trie_account);
        }
    }

    let root = state_root_unhashed(
        trie_accounts
            .into_iter()
            .map(|(addr, account)| (Address::from(addr), account)),
    );
    b256_to_bytes(root)
}

fn is_empty_trie_account(account: &TrieAccount) -> bool {
    account.nonce == 0
        && account.balance == U256::ZERO
        && account.storage_root == EMPTY_ROOT_HASH
        && account.code_hash == KECCAK_EMPTY
}

fn normalize_code_hash(code_hash: B256) -> B256 {
    if code_hash.is_zero() {
        KECCAK_EMPTY
    } else {
        code_hash
    }
}

fn b256_to_bytes(value: B256) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(value.as_ref());
    out
}

#[cfg(feature = "full_state_root")]
pub fn compute_full_state_root() -> [u8; 32] {
    with_state(compute_state_root_with)
}

pub fn compute_block_change_hash(tx_change_hashes: &[[u8; 32]]) -> [u8; 32] {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"ic-evm:block-changes:v1");
    for hash in tx_change_hashes.iter() {
        buf.extend_from_slice(hash);
    }
    keccak256(&buf)
}

pub fn empty_tx_change_hash() -> [u8; 32] {
    keccak256(b"ic-evm:tx-change:v1")
}
