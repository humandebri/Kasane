//! どこで: state_root計算層 / 何を: 差分更新 + 互換ルート計算 / なぜ: 全ストレージ走査を避けるため

mod node_codec;
mod node_store;
mod trie_update;

use crate::bytes::b256_to_bytes;
use crate::hash::keccak256;
use crate::revm_exec::StateDiff;
use alloy_primitives::{Address, B256, U256};
use alloy_rlp::Encodable;
use alloy_trie::nodes::{BranchNode, ExtensionNode, LeafNode, RlpNode};
use alloy_trie::root::{state_root_unhashed, storage_root_unhashed};
use alloy_trie::{Nibbles, TrieMask};
use alloy_trie::{TrieAccount, EMPTY_ROOT_HASH, KECCAK_EMPTY};
use evm_db::chain_data::{HashKey, MigrationPhase, NodeRecord};
use evm_db::stable_state::{clear_map as clear_stable_map, StableState};
use evm_db::types::keys::{make_account_key, make_storage_key, AccountKey};
use node_codec::rlp_node_to_root;
use node_store::{apply_journal, AnchorDelta, JournalUpdate};
use std::collections::BTreeMap;
use trie_update::{
    build_state_update_journal, build_state_update_journal_full, NewNodeRecords, NodeDeltaCounts,
};

pub const VERIFY_SAMPLE_MOD: u64 = 1024;
pub const VERIFY_MAX_TOUCHED_ACCOUNTS: u32 = 8;
pub const VERIFY_MAX_TOUCHED_SLOTS: u32 = 64;
const NODE_DB_REBUILD_ON_VERIFY_ONLY: bool = false;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TouchedSummary {
    pub accounts_count: u32,
    pub slots_count: u32,
    pub delta_digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageRootUpdate {
    pub addr: [u8; 20],
    pub storage_root: Option<[u8; 32]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedStateRoot {
    pub state_root: [u8; 32],
    pub storage_updates: Vec<StorageRootUpdate>,
    pub node_delta_counts: NodeDeltaCounts,
    pub new_node_records: NewNodeRecords,
    pub updated_account_leaf_hashes: BTreeMap<AccountKey, HashKey>,
    pub anchor_delta: AnchorDelta,
}

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

pub fn compute_state_root_incremental_with(
    state: &mut StableState,
    touched_addrs: &[[u8; 20]],
) -> [u8; 32] {
    ensure_initialized(state);
    for addr in touched_addrs {
        let root = compute_storage_root_for_address(state, *addr);
        let key = make_account_key(*addr);
        if root == EMPTY_ROOT_HASH {
            state.state_storage_roots.remove(&key);
        } else {
            state
                .state_storage_roots
                .insert(key, evm_db::types::values::U256Val(b256_to_bytes(root)));
        }
    }
    let root = compute_state_root_from_cache(state);
    let mut meta = *state.state_root_meta.get();
    meta.initialized = true;
    meta.state_root = root;
    state.state_root_meta.set(meta);
    root
}

pub fn prepare_state_root_commit(
    state: &mut StableState,
    state_diffs: &[StateDiff],
    touched_addrs: &[[u8; 20]],
    touched: TouchedSummary,
    block_number: u64,
    _parent_hash: [u8; 32],
    _timestamp: u64,
) -> Result<PreparedStateRoot, &'static str> {
    if state.state_root_migration.get().phase != MigrationPhase::Done {
        return Err("state_root_migration_pending");
    }
    ensure_initialized(state);
    let delta = build_trie_delta(state_diffs);
    if delta.accounts.is_empty()
        && touched_addrs.is_empty()
        && touched.accounts_count == 0
        && touched.slots_count == 0
    {
        let mut metrics = *state.state_root_metrics.get();
        metrics.state_root_verify_skipped_count =
            metrics.state_root_verify_skipped_count.saturating_add(1);
        metrics.migration_phase = state.state_root_migration.get().phase as u8;
        state.state_root_metrics.set(metrics);
        return Ok(PreparedStateRoot {
            state_root: state.state_root_meta.get().state_root,
            storage_updates: Vec::new(),
            node_delta_counts: BTreeMap::new(),
            new_node_records: BTreeMap::new(),
            updated_account_leaf_hashes: BTreeMap::new(),
            anchor_delta: AnchorDelta::default(),
        });
    }
    let effective_changes = !delta.accounts.is_empty() || !touched_addrs.is_empty();
    let built = if !NODE_DB_REBUILD_ON_VERIFY_ONLY || should_verify(block_number, touched) {
        build_state_update_journal(state, &delta, touched_addrs)
    } else {
        trie_update::TrieUpdateJournal {
            state_root: state.state_root_meta.get().state_root,
            storage_updates: Vec::new(),
            node_delta_counts: BTreeMap::new(),
            new_node_records: BTreeMap::new(),
            updated_account_leaf_hashes: BTreeMap::new(),
            anchor_delta: AnchorDelta::default(),
        }
    };
    let candidate_root = if effective_changes {
        built.state_root
    } else {
        state.state_root_meta.get().state_root
    };

    let mut metrics = *state.state_root_metrics.get();
    let verify_now = should_verify(block_number, touched);
    if verify_now {
        metrics.state_root_verify_count = metrics.state_root_verify_count.saturating_add(1);
    } else {
        metrics.state_root_verify_skipped_count =
            metrics.state_root_verify_skipped_count.saturating_add(1);
    }
    metrics.migration_phase = state.state_root_migration.get().phase as u8;
    state.state_root_metrics.set(metrics);
    Ok(PreparedStateRoot {
        state_root: candidate_root,
        storage_updates: built.storage_updates,
        node_delta_counts: built.node_delta_counts,
        new_node_records: built.new_node_records,
        updated_account_leaf_hashes: built.updated_account_leaf_hashes,
        anchor_delta: built.anchor_delta,
    })
}

pub fn apply_state_root_commit(state: &mut StableState, prepared: PreparedStateRoot) {
    let current_root = state.state_root_meta.get().state_root;
    if prepared.storage_updates.is_empty()
        && prepared.node_delta_counts.is_empty()
        && prepared.new_node_records.is_empty()
        && prepared.updated_account_leaf_hashes.is_empty()
        && prepared.state_root == current_root
    {
        return;
    }

    for update in prepared.storage_updates {
        let key = make_account_key(update.addr);
        if let Some(root) = update.storage_root {
            state
                .state_storage_roots
                .insert(key, evm_db::types::values::U256Val(root));
        } else {
            state.state_storage_roots.remove(&key);
        }
    }
    let mut meta = *state.state_root_meta.get();
    meta.initialized = true;
    meta.state_root = prepared.state_root;
    state.state_root_meta.set(meta);
    apply_journal(
        state,
        JournalUpdate {
            node_delta_counts: prepared.node_delta_counts,
            new_node_records: prepared.new_node_records,
            anchor_delta: prepared.anchor_delta,
        },
    );
    if !prepared.updated_account_leaf_hashes.is_empty() {
        clear_stable_map(&mut state.state_root_account_leaf_hash);
        for (key, hash) in prepared.updated_account_leaf_hashes {
            if state
                .state_root_node_db
                .get(&hash)
                .map(|record| record.refcnt > 0)
                .unwrap_or(false)
            {
                state.state_root_account_leaf_hash.insert(key, hash);
            }
        }
    }
}

pub fn commit_state_root_with(
    state: &mut StableState,
    touched_addrs: &[[u8; 20]],
    touched: TouchedSummary,
    block_number: u64,
    parent_hash: [u8; 32],
    timestamp: u64,
) -> Result<[u8; 32], &'static str> {
    let prepared = prepare_state_root_commit(
        state,
        &[],
        touched_addrs,
        touched,
        block_number,
        parent_hash,
        timestamp,
    )?;
    let root = prepared.state_root;
    apply_state_root_commit(state, prepared);
    Ok(root)
}

pub fn current_state_root_with(state: &mut StableState) -> [u8; 32] {
    ensure_initialized(state);
    state.state_root_meta.get().state_root
}

pub fn run_migration_tick(state: &mut StableState, max_steps: u32) -> bool {
    let max_steps = max_steps.max(1);
    let mut migration = *state.state_root_migration.get();
    match migration.phase {
        MigrationPhase::Done => return true,
        MigrationPhase::Init => {
            migration.phase = MigrationPhase::BuildTrie;
            migration.cursor = 0;
            state.state_root_migration.set(migration);
            return false;
        }
        MigrationPhase::BuildTrie => {
            if migration.cursor == 0 {
                clear_stable_map(&mut state.state_storage_roots);
            }
            let start = usize::try_from(migration.cursor).unwrap_or(usize::MAX);
            let limit = usize::try_from(max_steps).unwrap_or(usize::MAX);
            let mut scanned = 0usize;
            let mut last_addr: Option<[u8; 20]> = None;
            let mut addrs = Vec::new();
            for entry in state.storage.iter().skip(start).take(limit) {
                scanned = scanned.saturating_add(1);
                let key = entry.key().0;
                if key[0] != 0x02 {
                    continue;
                }
                let mut addr = [0u8; 20];
                addr.copy_from_slice(&key[1..21]);
                if last_addr == Some(addr) {
                    continue;
                }
                last_addr = Some(addr);
                addrs.push(addr);
            }
            for addr in addrs.iter().copied() {
                let root = compute_storage_root_for_address(state, addr);
                let key = make_account_key(addr);
                if root == EMPTY_ROOT_HASH {
                    state.state_storage_roots.remove(&key);
                } else {
                    state
                        .state_storage_roots
                        .insert(key, evm_db::types::values::U256Val(b256_to_bytes(root)));
                }
            }
            if scanned < limit {
                migration.phase = MigrationPhase::BuildRefcnt;
                migration.cursor = 0;
            } else {
                migration.cursor = u64::try_from(start.saturating_add(scanned)).unwrap_or(u64::MAX);
            }
            state.state_root_migration.set(migration);
            return false;
        }
        MigrationPhase::BuildRefcnt => {
            clear_stable_map(&mut state.state_root_node_db);
            clear_stable_map(&mut state.state_root_account_leaf_hash);
            clear_stable_map(&mut state.state_root_gc_queue);
            state
                .state_root_gc_state
                .set(evm_db::chain_data::GcStateV1::new());
            let built = build_state_update_journal_full(state, &TrieDelta::default(), Vec::new());
            apply_journal(
                state,
                JournalUpdate {
                    node_delta_counts: built.node_delta_counts,
                    new_node_records: built.new_node_records,
                    anchor_delta: built.anchor_delta,
                },
            );
            clear_stable_map(&mut state.state_root_account_leaf_hash);
            for (key, hash) in built.updated_account_leaf_hashes {
                state.state_root_account_leaf_hash.insert(key, hash);
            }
            migration.phase = MigrationPhase::Verify;
            migration.cursor = 0;
            state.state_root_migration.set(migration);
            return false;
        }
        MigrationPhase::Verify => {
            let root = compute_state_root_from_cache(state);
            let mut meta = *state.state_root_meta.get();
            meta.initialized = true;
            meta.state_root = root;
            state.state_root_meta.set(meta);
            migration.phase = MigrationPhase::Done;
            migration.cursor = 0;
            migration.last_error = 0;
            state.state_root_migration.set(migration);
            return true;
        }
    }
}

#[derive(Clone, Debug, Default)]
struct AccountDelta {
    deleted: bool,
    nonce: u64,
    balance: [u8; 32],
    code_hash: [u8; 32],
    storage: BTreeMap<[u8; 32], Option<[u8; 32]>>,
}

#[derive(Clone, Debug, Default)]
struct TrieDelta {
    accounts: BTreeMap<[u8; 20], AccountDelta>,
}

fn build_trie_delta(state_diffs: &[StateDiff]) -> TrieDelta {
    let mut delta = TrieDelta::default();
    for state_diff in state_diffs {
        for (address, account) in state_diff.iter() {
            let mut addr = [0u8; 20];
            addr.copy_from_slice(address.as_ref());
            let account_delta = delta.accounts.entry(addr).or_default();
            let deleted =
                account.is_selfdestructed() || (account.is_empty() && account.is_touched());
            account_delta.deleted = deleted;
            account_delta.nonce = account.info.nonce;
            account_delta.balance = account.info.balance.to_be_bytes();
            account_delta.code_hash = b256_to_bytes(account.info.code_hash);
            for (slot, entry) in account.changed_storage_slots() {
                let slot_bytes = slot.to_be_bytes::<32>();
                if entry.present_value.is_zero() {
                    account_delta.storage.insert(slot_bytes, None);
                } else {
                    account_delta
                        .storage
                        .insert(slot_bytes, Some(entry.present_value.to_be_bytes::<32>()));
                }
            }
        }
    }
    delta
}

#[allow(dead_code)]
fn compute_state_root_fullscan_overlay(state: &StableState, delta: &TrieDelta) -> [u8; 32] {
    let mut storage_by_addr: BTreeMap<[u8; 20], BTreeMap<[u8; 32], [u8; 32]>> = BTreeMap::new();
    for addr in delta.accounts.keys() {
        let lower = make_storage_key(*addr, [0u8; 32]);
        let upper = make_storage_key(*addr, [0xffu8; 32]);
        let mut slots: BTreeMap<[u8; 32], [u8; 32]> = BTreeMap::new();
        for entry in state.storage.range(lower..=upper) {
            let key = entry.key().0;
            if key[0] != 0x02 || key[1..21] != addr[..] {
                break;
            }
            let mut slot = [0u8; 32];
            slot.copy_from_slice(&key[21..53]);
            slots.insert(slot, entry.value().0);
        }
        if !slots.is_empty() {
            storage_by_addr.insert(*addr, slots);
        }
    }
    for (addr, account_delta) in delta.accounts.iter() {
        let slots = storage_by_addr.entry(*addr).or_default();
        if account_delta.deleted {
            slots.clear();
            continue;
        }
        for (slot, value) in account_delta.storage.iter() {
            if let Some(value_bytes) = value {
                slots.insert(*slot, *value_bytes);
            } else {
                slots.remove(slot);
            }
        }
    }

    let mut trie_accounts: BTreeMap<[u8; 20], TrieAccount> = BTreeMap::new();
    for entry in state.accounts.iter() {
        let key = entry.key().0;
        if key[0] != 0x01 {
            continue;
        }
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&key[1..21]);
        let mut nonce = entry.value().nonce();
        let mut balance = U256::from_be_bytes(entry.value().balance());
        let mut code_hash = normalize_code_hash(B256::from(entry.value().code_hash()));
        let mut deleted = false;
        if let Some(account_delta) = delta.accounts.get(&addr) {
            deleted = account_delta.deleted;
            nonce = account_delta.nonce;
            balance = U256::from_be_bytes(account_delta.balance);
            code_hash = normalize_code_hash(B256::from(account_delta.code_hash));
        }
        if deleted {
            continue;
        }
        let storage_root = if delta.accounts.contains_key(&addr) {
            storage_by_addr
                .get(&addr)
                .map(|slots| {
                    storage_root_unhashed(
                        slots
                            .iter()
                            .map(|(slot, value)| (B256::from(*slot), U256::from_be_bytes(*value))),
                    )
                })
                .unwrap_or(EMPTY_ROOT_HASH)
        } else {
            state
                .state_storage_roots
                .get(&make_account_key(addr))
                .map(|value| B256::from(value.0))
                .unwrap_or_else(|| compute_storage_root_for_address(state, addr))
        };
        let trie_account = TrieAccount {
            nonce,
            balance,
            storage_root,
            code_hash,
        };
        if !is_empty_trie_account(&trie_account) {
            trie_accounts.insert(addr, trie_account);
        }
    }
    for (addr, account_delta) in delta.accounts.iter() {
        if trie_accounts.contains_key(addr) || account_delta.deleted {
            continue;
        }
        let storage_root = storage_by_addr
            .get(addr)
            .map(|slots| {
                storage_root_unhashed(
                    slots
                        .iter()
                        .map(|(slot, value)| (B256::from(*slot), U256::from_be_bytes(*value))),
                )
            })
            .unwrap_or(EMPTY_ROOT_HASH);
        let trie_account = TrieAccount {
            nonce: account_delta.nonce,
            balance: U256::from_be_bytes(account_delta.balance),
            storage_root,
            code_hash: normalize_code_hash(B256::from(account_delta.code_hash)),
        };
        if !is_empty_trie_account(&trie_account) {
            trie_accounts.insert(*addr, trie_account);
        }
    }
    for (addr, slots) in storage_by_addr {
        if trie_accounts.contains_key(&addr) {
            continue;
        }
        let storage_root = storage_root_unhashed(
            slots
                .into_iter()
                .map(|(slot, value)| (B256::from(slot), U256::from_be_bytes(value))),
        );
        let trie_account = TrieAccount {
            nonce: 0,
            balance: U256::ZERO,
            storage_root,
            code_hash: KECCAK_EMPTY,
        };
        if !is_empty_trie_account(&trie_account) {
            trie_accounts.insert(addr, trie_account);
        }
    }
    b256_to_bytes(state_root_unhashed(
        trie_accounts
            .into_iter()
            .map(|(addr, account)| (Address::from(addr), account)),
    ))
}

#[derive(Clone)]
struct TrieEntry {
    key_nibbles: [u8; 64],
    value: Vec<u8>,
    account_addr: Option<[u8; 20]>,
}

#[derive(Clone)]
struct BuiltNode {
    ptr: RlpNode,
    raw_rlp: Vec<u8>,
}

fn build_state_update_journal_overlay(
    state: &StableState,
    delta: &TrieDelta,
    storage_updates: Vec<StorageRootUpdate>,
) -> trie_update::TrieUpdateJournal {
    let (accounts, storage_by_addr) = collect_overlay_accounts_and_storage(state, delta);
    let mut node_map: BTreeMap<HashKey, NodeRecord> = BTreeMap::new();
    let mut account_leaf_hashes: BTreeMap<AccountKey, HashKey> = BTreeMap::new();
    let mut forced_roots: BTreeMap<HashKey, Vec<u8>> = BTreeMap::new();

    for slots in storage_by_addr.values() {
        if slots.is_empty() {
            continue;
        }
        let mut storage_entries = Vec::with_capacity(slots.len());
        for (slot, value) in slots {
            let mut value_rlp = Vec::with_capacity(33);
            U256::from_be_bytes(*value).encode(&mut value_rlp);
            storage_entries.push(TrieEntry {
                key_nibbles: bytes_to_nibbles(keccak256(slot)),
                value: value_rlp,
                account_addr: None,
            });
        }
        storage_entries.sort_by_key(|entry| entry.key_nibbles);
        if let Some(storage_root_node) =
            build_trie_nodes(&storage_entries, 0, &mut node_map, &mut account_leaf_hashes)
        {
            let storage_root = HashKey(b256_to_bytes(rlp_node_to_root(storage_root_node.ptr)));
            if storage_root_node.raw_rlp.len() < 32 {
                forced_roots.insert(storage_root, storage_root_node.raw_rlp);
            }
        }
    }

    let mut account_entries = Vec::with_capacity(accounts.len());
    for (addr, account) in accounts {
        let mut value_rlp = Vec::with_capacity(128);
        account.encode(&mut value_rlp);
        account_entries.push(TrieEntry {
            key_nibbles: bytes_to_nibbles(keccak256(&addr)),
            value: value_rlp,
            account_addr: Some(addr),
        });
    }
    account_entries.sort_by_key(|entry| entry.key_nibbles);
    let root_node = build_trie_nodes(&account_entries, 0, &mut node_map, &mut account_leaf_hashes);
    let root = root_node
        .as_ref()
        .map(|node| rlp_node_to_root(node.ptr.clone()))
        .unwrap_or(EMPTY_ROOT_HASH);
    if let Some(node) = root_node {
        if node.raw_rlp.len() < 32 {
            forced_roots.insert(HashKey(b256_to_bytes(root)), node.raw_rlp);
        }
    }

    let mut new_node_records: BTreeMap<HashKey, Vec<u8>> = forced_roots;

    let mut node_delta_counts: NodeDeltaCounts = BTreeMap::new();
    let mut before_iter = state.state_root_node_db.iter();
    let mut after_iter = node_map.iter();
    let mut new_iter = new_node_records.iter();
    let mut before = before_iter.next();
    let mut after = after_iter.next();
    let mut new_only = new_iter.next();
    while before.is_some() || after.is_some() || new_only.is_some() {
        let mut next_key = None;
        if let Some(entry) = before.as_ref() {
            next_key = Some(*entry.key());
        }
        if let Some((key, _)) = after {
            let key = *key;
            next_key = Some(match next_key {
                Some(current) if current <= key => current,
                _ => key,
            });
        }
        if let Some((key, _)) = new_only {
            let key = *key;
            next_key = Some(match next_key {
                Some(current) if current <= key => current,
                _ => key,
            });
        }
        let key = match next_key {
            Some(value) => value,
            None => break,
        };
        let before_count = match before.as_ref() {
            Some(entry) if *entry.key() == key => i64::from(entry.value().refcnt),
            _ => 0,
        };
        let after_count = match after {
            Some((entry_key, entry_value)) if *entry_key == key => i64::from(entry_value.refcnt),
            _ => 0,
        };
        let diff = after_count - before_count;
        if diff != 0 {
            node_delta_counts.insert(key, diff);
        }
        if let Some(entry) = before.as_ref() {
            if *entry.key() == key {
                before = before_iter.next();
            }
        }
        if let Some((entry_key, _)) = after {
            if *entry_key == key {
                after = after_iter.next();
            }
        }
        if let Some((entry_key, _)) = new_only {
            if *entry_key == key {
                new_only = new_iter.next();
            }
        }
    }
    for (hash, record) in node_map.into_iter() {
        new_node_records.insert(hash, record.rlp);
    }

    let anchor_delta = build_anchor_delta(state, &storage_updates, b256_to_bytes(root));

    trie_update::TrieUpdateJournal {
        state_root: b256_to_bytes(root),
        storage_updates,
        node_delta_counts,
        new_node_records,
        updated_account_leaf_hashes: account_leaf_hashes,
        anchor_delta,
    }
}

fn collect_overlay_accounts_and_storage(
    state: &StableState,
    delta: &TrieDelta,
) -> (
    BTreeMap<[u8; 20], TrieAccount>,
    BTreeMap<[u8; 20], BTreeMap<[u8; 32], [u8; 32]>>,
) {
    let target_addrs = collect_overlay_target_addrs(state, delta);

    let mut storage_by_addr: BTreeMap<[u8; 20], BTreeMap<[u8; 32], [u8; 32]>> = BTreeMap::new();
    for addr in target_addrs.iter().copied() {
        let lower = make_storage_key(addr, [0u8; 32]);
        let upper = make_storage_key(addr, [0xffu8; 32]);
        let mut slots = BTreeMap::new();
        for entry in state.storage.range(lower..=upper) {
            let key = entry.key().0;
            if key[1..21] != addr {
                break;
            }
            let mut slot = [0u8; 32];
            slot.copy_from_slice(&key[21..53]);
            slots.insert(slot, entry.value().0);
        }
        if slots.is_empty() {
            continue;
        }
        storage_by_addr.insert(addr, slots);
    }
    for (addr, account_delta) in &delta.accounts {
        let slots = storage_by_addr.entry(*addr).or_default();
        if account_delta.deleted {
            slots.clear();
            continue;
        }
        for (slot, value) in &account_delta.storage {
            if let Some(value_bytes) = value {
                slots.insert(*slot, *value_bytes);
            } else {
                slots.remove(slot);
            }
        }
    }
    storage_by_addr.retain(|_, slots| !slots.is_empty());

    let mut trie_accounts: BTreeMap<[u8; 20], TrieAccount> = BTreeMap::new();
    for addr in target_addrs {
        let account_key = make_account_key(addr);
        let mut has_account = false;
        let mut nonce = 0u64;
        let mut balance = U256::ZERO;
        let mut code_hash = KECCAK_EMPTY;

        if let Some(account) = state.accounts.get(&account_key) {
            has_account = true;
            nonce = account.nonce();
            balance = U256::from_be_bytes(account.balance());
            code_hash = normalize_code_hash(B256::from(account.code_hash()));
        }
        if let Some(account_delta) = delta.accounts.get(&addr) {
            if account_delta.deleted {
                continue;
            }
            has_account = true;
            nonce = account_delta.nonce;
            balance = U256::from_be_bytes(account_delta.balance);
            code_hash = normalize_code_hash(B256::from(account_delta.code_hash));
        }

        if !has_account {
            if let Some(root) = state.state_storage_roots.get(&account_key) {
                let trie_account = TrieAccount {
                    nonce: 0,
                    balance: U256::ZERO,
                    storage_root: B256::from(root.0),
                    code_hash: KECCAK_EMPTY,
                };
                if !is_empty_trie_account(&trie_account) {
                    trie_accounts.insert(addr, trie_account);
                }
            }
            continue;
        }

        let storage_root = storage_by_addr
            .get(&addr)
            .map(|slots| {
                storage_root_unhashed(
                    slots
                        .iter()
                        .map(|(slot, value)| (B256::from(*slot), U256::from_be_bytes(*value))),
                )
            })
            .or_else(|| {
                state
                    .state_storage_roots
                    .get(&account_key)
                    .map(|entry| B256::from(entry.0))
            })
            .unwrap_or(EMPTY_ROOT_HASH);

        let trie_account = TrieAccount {
            nonce,
            balance,
            storage_root,
            code_hash,
        };
        if !is_empty_trie_account(&trie_account) {
            trie_accounts.insert(addr, trie_account);
        }
    }
    (trie_accounts, storage_by_addr)
}

fn collect_overlay_target_addrs(
    state: &StableState,
    delta: &TrieDelta,
) -> std::collections::BTreeSet<[u8; 20]> {
    let mut addrs = std::collections::BTreeSet::new();

    for entry in state.accounts.iter() {
        let key = entry.key().0;
        if key[0] != 0x01 {
            continue;
        }
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&key[1..21]);
        addrs.insert(addr);
    }
    for entry in state.state_storage_roots.iter() {
        let key = entry.key().0;
        if key[0] != 0x01 {
            continue;
        }
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&key[1..21]);
        addrs.insert(addr);
    }
    for addr in delta.accounts.keys().copied() {
        addrs.insert(addr);
    }

    addrs
}

fn build_trie_nodes(
    entries: &[TrieEntry],
    depth: usize,
    node_map: &mut BTreeMap<HashKey, NodeRecord>,
    account_leaf_hashes: &mut BTreeMap<AccountKey, HashKey>,
) -> Option<BuiltNode> {
    if entries.is_empty() {
        return None;
    }
    if entries.len() == 1 {
        let key = Nibbles::from_nibbles_unchecked(&entries[0].key_nibbles[depth..]);
        let leaf = LeafNode::new(key, entries[0].value.clone());
        let mut rlp = Vec::with_capacity(96);
        let ptr = leaf.as_ref().rlp(&mut rlp);
        let hash = record_hashed_node(node_map, &rlp);
        if let Some(addr) = entries[0].account_addr {
            account_leaf_hashes.insert(make_account_key(addr), hash);
        }
        return Some(BuiltNode { ptr, raw_rlp: rlp });
    }

    let mut common_end = depth;
    while common_end < 64 {
        let nib = entries[0].key_nibbles[common_end];
        if entries
            .iter()
            .all(|entry| entry.key_nibbles[common_end] == nib)
        {
            common_end = common_end.saturating_add(1);
        } else {
            break;
        }
    }
    if common_end > depth {
        let child = build_trie_nodes(entries, common_end, node_map, account_leaf_hashes)?;
        let key = Nibbles::from_nibbles_unchecked(&entries[0].key_nibbles[depth..common_end]);
        let extension = ExtensionNode::new(key, child.ptr);
        let mut rlp = Vec::with_capacity(96);
        let ptr = extension.as_ref().rlp(&mut rlp);
        record_hashed_node(node_map, &rlp);
        return Some(BuiltNode { ptr, raw_rlp: rlp });
    }

    let mut stack = Vec::with_capacity(16);
    let mut mask = TrieMask::default();
    let mut cursor = 0usize;
    while cursor < entries.len() {
        let nib = entries[cursor].key_nibbles[depth];
        let start = cursor;
        cursor = cursor.saturating_add(1);
        while cursor < entries.len() && entries[cursor].key_nibbles[depth] == nib {
            cursor = cursor.saturating_add(1);
        }
        if let Some(child) = build_trie_nodes(
            &entries[start..cursor],
            depth.saturating_add(1),
            node_map,
            account_leaf_hashes,
        ) {
            stack.push(child.ptr);
            mask.set_bit(nib);
        }
    }
    let branch = BranchNode::new(stack, mask);
    let mut rlp = Vec::with_capacity(96);
    let ptr = branch.as_ref().rlp(&mut rlp);
    record_hashed_node(node_map, &rlp);
    Some(BuiltNode { ptr, raw_rlp: rlp })
}

fn record_hashed_node(node_map: &mut BTreeMap<HashKey, NodeRecord>, rlp: &[u8]) -> HashKey {
    let key = HashKey(keccak256(rlp));
    if rlp.len() < 32 {
        return key;
    }
    if let Some(existing) = node_map.get_mut(&key) {
        existing.refcnt = existing.refcnt.saturating_add(1);
        return key;
    }
    node_map.insert(key, NodeRecord::new(1, rlp.to_vec()));
    key
}

fn bytes_to_nibbles(bytes: [u8; 32]) -> [u8; 64] {
    let mut nibbles = [0u8; 64];
    for (i, byte) in bytes.iter().enumerate() {
        nibbles[i * 2] = byte >> 4;
        nibbles[i * 2 + 1] = byte & 0x0f;
    }
    nibbles
}

fn build_anchor_delta(
    state: &StableState,
    storage_updates: &[StorageRootUpdate],
    new_state_root: [u8; 32],
) -> AnchorDelta {
    let mut out = AnchorDelta::default();
    out.state_root_old = node_codec::root_hash_key(state.state_root_meta.get().state_root);
    out.state_root_new = node_codec::root_hash_key(new_state_root);
    for update in storage_updates {
        let key = make_account_key(update.addr);
        let old_root = state.state_storage_roots.get(&key).map(|v| HashKey(v.0));
        let new_root = update.storage_root.map(HashKey);
        if let Some(hash) = old_root {
            out.storage_root_old.push(hash);
        }
        if let Some(hash) = new_root {
            out.storage_root_new.push(hash);
        }
    }
    out
}

#[cfg(test)]
fn apply_node_db_records(state: &mut StableState, records: Vec<(HashKey, NodeRecord)>) {
    use std::collections::BTreeSet;

    let mut next: BTreeMap<HashKey, NodeRecord> = BTreeMap::new();
    for (key, record) in records {
        if keccak256(&record.rlp) != key.0 {
            continue;
        }
        next.insert(key, record);
    }
    let mut counts: NodeDeltaCounts = BTreeMap::new();
    let mut new_records: NewNodeRecords = BTreeMap::new();
    let mut all_keys: BTreeSet<HashKey> = BTreeSet::new();
    for key in state.state_root_node_db.iter().map(|e| *e.key()) {
        all_keys.insert(key);
    }
    for key in next.keys().copied() {
        all_keys.insert(key);
    }
    for key in all_keys {
        let before = state
            .state_root_node_db
            .get(&key)
            .map(|v| i64::from(v.refcnt))
            .unwrap_or(0);
        let after = next.get(&key).map(|v| i64::from(v.refcnt)).unwrap_or(0);
        let diff = after - before;
        if diff != 0 {
            counts.insert(key, diff);
        }
        if let Some(record) = next.get(&key) {
            new_records.insert(key, record.rlp.clone());
        }
    }
    apply_journal(
        state,
        JournalUpdate {
            node_delta_counts: counts,
            new_node_records: new_records,
            anchor_delta: AnchorDelta::default(),
        },
    );
}

fn should_verify(block_number: u64, touched: TouchedSummary) -> bool {
    if block_number % VERIFY_SAMPLE_MOD == 0 {
        return true;
    }
    touched.accounts_count <= VERIFY_MAX_TOUCHED_ACCOUNTS
        && touched.slots_count <= VERIFY_MAX_TOUCHED_SLOTS
}

fn ensure_initialized(state: &mut StableState) {
    if state.state_root_meta.get().initialized {
        ensure_node_db_bootstrapped(state);
        return;
    }
    rebuild_storage_root_cache(state);
    let root = compute_state_root_from_cache(state);
    let mut meta = *state.state_root_meta.get();
    meta.initialized = true;
    meta.state_root = root;
    state.state_root_meta.set(meta);
    ensure_node_db_bootstrapped(state);
}

fn ensure_node_db_bootstrapped(state: &mut StableState) {
    let current_root = state.state_root_meta.get().state_root;
    if current_root == b256_to_bytes(EMPTY_ROOT_HASH) {
        return;
    }
    if state
        .state_root_node_db
        .get(&HashKey(current_root))
        .is_some()
    {
        return;
    }
    let built = build_state_update_journal_full(state, &TrieDelta::default(), Vec::new());
    apply_journal(
        state,
        JournalUpdate {
            node_delta_counts: built.node_delta_counts,
            new_node_records: built.new_node_records,
            anchor_delta: built.anchor_delta,
        },
    );
    clear_stable_map(&mut state.state_root_account_leaf_hash);
    for (key, hash) in built.updated_account_leaf_hashes {
        state.state_root_account_leaf_hash.insert(key, hash);
    }
}

fn rebuild_storage_root_cache(state: &mut StableState) {
    let mut by_addr: BTreeMap<[u8; 20], Vec<(B256, U256)>> = BTreeMap::new();
    for entry in state.storage.iter() {
        let key = entry.key().0;
        if key[0] != 0x02 {
            continue;
        }
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&key[1..21]);
        let mut slot = [0u8; 32];
        slot.copy_from_slice(&key[21..53]);
        by_addr
            .entry(addr)
            .or_default()
            .push((B256::from(slot), U256::from_be_bytes(entry.value().0)));
    }
    let keys: Vec<_> = state
        .state_storage_roots
        .iter()
        .map(|entry| *entry.key())
        .collect();
    for key in keys {
        state.state_storage_roots.remove(&key);
    }
    for (addr, slots) in by_addr {
        let root = storage_root_unhashed(slots);
        if root != EMPTY_ROOT_HASH {
            state.state_storage_roots.insert(
                make_account_key(addr),
                evm_db::types::values::U256Val(b256_to_bytes(root)),
            );
        }
    }
}

fn compute_storage_root_for_address(state: &StableState, addr: [u8; 20]) -> B256 {
    let lower = make_storage_key(addr, [0u8; 32]);
    let upper = make_storage_key(addr, [0xffu8; 32]);
    let mut slots = Vec::new();
    for entry in state.storage.range(lower..=upper) {
        let key = entry.key().0;
        if key[1..21] != addr[..] {
            break;
        }
        let mut slot = [0u8; 32];
        slot.copy_from_slice(&key[21..53]);
        slots.push((B256::from(slot), U256::from_be_bytes(entry.value().0)));
    }
    if slots.is_empty() {
        return EMPTY_ROOT_HASH;
    }
    storage_root_unhashed(slots)
}

fn compute_state_root_from_cache(state: &StableState) -> [u8; 32] {
    let mut trie_accounts: BTreeMap<[u8; 20], TrieAccount> = BTreeMap::new();
    for entry in state.accounts.iter() {
        let key = entry.key().0;
        if key[0] != 0x01 {
            continue;
        }
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&key[1..21]);
        let account = entry.value();
        let storage_root = state
            .state_storage_roots
            .get(&make_account_key(addr))
            .map(|value| B256::from(value.0))
            .unwrap_or(EMPTY_ROOT_HASH);
        let trie_account = TrieAccount {
            nonce: account.nonce(),
            balance: U256::from_be_bytes(account.balance()),
            storage_root,
            code_hash: normalize_code_hash(B256::from(account.code_hash())),
        };
        if !is_empty_trie_account(&trie_account) {
            trie_accounts.insert(addr, trie_account);
        }
    }
    for entry in state.state_storage_roots.iter() {
        let key = entry.key().0;
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&key[1..21]);
        if trie_accounts.contains_key(&addr) {
            continue;
        }
        let trie_account = TrieAccount {
            nonce: 0,
            balance: U256::ZERO,
            storage_root: B256::from(entry.value().0),
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

#[cfg(test)]
mod tests {
    use super::*;
    use evm_db::chain_data::NodeRecord;
    use evm_db::stable_state::{init_stable_state, with_state_mut};
    use evm_db::types::keys::make_storage_key;
    use evm_db::types::values::U256Val;

    #[test]
    fn node_db_refcnt_and_gc_follow_records() {
        init_stable_state();
        with_state_mut(|state| {
            let rlp1 = vec![0x80];
            let rlp2 = vec![0x81];
            let k1 = HashKey(keccak256(&rlp1));
            let k2 = HashKey(keccak256(&rlp2));

            apply_node_db_records(
                state,
                vec![
                    (k1, NodeRecord::new(2, rlp1.clone())),
                    (k2, NodeRecord::new(1, rlp2.clone())),
                ],
            );
            assert_eq!(state.state_root_node_db.len(), 2);
            assert_eq!(state.state_root_node_db.get(&k1).map(|r| r.refcnt), Some(2));
            assert_eq!(state.state_root_node_db.get(&k2).map(|r| r.refcnt), Some(1));
            assert_eq!(state.state_root_metrics.get().node_db_reachable, 2);
            assert_eq!(state.state_root_metrics.get().node_db_unreachable, 0);

            apply_node_db_records(state, vec![(k1, NodeRecord::new(1, rlp1))]);
            assert_eq!(state.state_root_node_db.len(), 1);
            assert_eq!(state.state_root_node_db.get(&k1).map(|r| r.refcnt), Some(1));
            assert!(state.state_root_node_db.get(&k2).is_none());
            assert_eq!(state.state_root_metrics.get().node_db_reachable, 1);
            assert_eq!(state.state_root_metrics.get().node_db_unreachable, 0);
        });
    }

    #[test]
    fn node_db_rejects_invalid_hash_record() {
        init_stable_state();
        with_state_mut(|state| {
            let invalid = HashKey([9u8; 32]);
            apply_node_db_records(state, vec![(invalid, NodeRecord::new(1, vec![0x80]))]);
            assert_eq!(state.state_root_node_db.len(), 0);
        });
    }

    #[test]
    fn account_leaf_hash_index_skips_dangling_hashes() {
        init_stable_state();
        with_state_mut(|state| {
            let addr = [0x11u8; 20];
            let rlp = vec![0x80];
            let valid_hash = HashKey(keccak256(&rlp));
            let dangling_hash = HashKey([0x77u8; 32]);
            apply_state_root_commit(
                state,
                PreparedStateRoot {
                    state_root: [0u8; 32],
                    storage_updates: Vec::new(),
                    node_delta_counts: BTreeMap::from([(valid_hash, 1)]),
                    new_node_records: BTreeMap::from([(valid_hash, rlp)]),
                    updated_account_leaf_hashes: BTreeMap::from([
                        (make_account_key(addr), valid_hash),
                        (make_account_key([0x22u8; 20]), dangling_hash),
                    ]),
                    anchor_delta: AnchorDelta::default(),
                },
            );
            assert_eq!(
                state
                    .state_root_account_leaf_hash
                    .get(&make_account_key(addr))
                    .map(|h| h.0),
                Some(valid_hash.0)
            );
            assert!(state
                .state_root_account_leaf_hash
                .get(&make_account_key([0x22u8; 20]))
                .is_none());
        });
    }

    #[test]
    fn journal_includes_delta_only_addresses() {
        init_stable_state();
        with_state_mut(|state| {
            let addr = [0x33u8; 20];
            let slot = [0x01u8; 32];
            state
                .storage
                .insert(make_storage_key(addr, slot), U256Val::new([0x11u8; 32]));

            let mut delta = TrieDelta::default();
            let account = delta.accounts.entry(addr).or_default();
            account.storage.insert(slot, Some([0x22u8; 32]));

            let journal = super::trie_update::build_state_update_journal(state, &delta, &[]);
            assert_eq!(journal.storage_updates.len(), 1);
            assert_eq!(journal.storage_updates[0].addr, addr);
            assert!(journal.storage_updates[0].storage_root.is_some());
        });
    }
}
