//! どこで: state_root計算層 / 何を: 差分更新 + 互換ルート計算 / なぜ: 全ストレージ走査を避けるため

mod node_codec;
mod node_store;
mod trie_update;

use crate::bytes::b256_to_bytes;
use crate::hash::keccak256;
use crate::revm_exec::StateDiff;
use alloy_primitives::{Address, B256, U256};
use alloy_trie::root::{state_root_unhashed, storage_root_unhashed};
use alloy_trie::{TrieAccount, EMPTY_ROOT_HASH, KECCAK_EMPTY};
use evm_db::chain_data::{HashKey, MigrationPhase};
use evm_db::stable_state::{clear_map as clear_stable_map, StableState};
use evm_db::types::keys::{
    make_account_key, make_storage_key, parse_account_key_bytes, parse_storage_key_bytes,
    AccountKey,
};
use node_store::{apply_journal, AnchorDelta, JournalUpdate};
use std::collections::BTreeMap;
use trie_update::{
    build_state_update_journal, build_state_update_journal_full, NewNodeRecords, NodeDeltaCounts,
};

pub const VERIFY_SAMPLE_MOD: u64 = 1024;
pub const VERIFY_MAX_TOUCHED_ACCOUNTS: u32 = 8;
pub const VERIFY_MAX_TOUCHED_SLOTS: u32 = 64;
const NODE_DB_REBUILD_ON_VERIFY_ONLY: bool = false;
const INIT_MIGRATION_TICKS_PER_CALL: u32 = 8;
const INIT_MIGRATION_MAX_STEPS: u32 = 512;

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
    ensure_initialized(state);
    if !state.state_root_meta.get().initialized
        || state.state_root_migration.get().phase != MigrationPhase::Done
    {
        return Err("state_root_migration_pending");
    }
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
        MigrationPhase::Done => {
            if state.state_root_meta.get().initialized {
                return true;
            }
            migration.phase = MigrationPhase::Init;
            migration.cursor = 0;
            migration.last_error = 0;
            state.state_root_migration.set(migration);
            false
        }
        MigrationPhase::Init => {
            migration.phase = MigrationPhase::BuildTrie;
            migration.cursor = 0;
            state.state_root_migration.set(migration);
            false
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
                let Some((addr, _slot)) = parse_storage_key_bytes(&key) else {
                    continue;
                };
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
            false
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
            false
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
            true
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

fn build_anchor_delta(
    state: &StableState,
    storage_updates: &[StorageRootUpdate],
    new_state_root: [u8; 32],
) -> AnchorDelta {
    let mut out = AnchorDelta {
        state_root_old: node_codec::root_hash_key(state.state_root_meta.get().state_root),
        state_root_new: node_codec::root_hash_key(new_state_root),
        ..AnchorDelta::default()
    };
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

fn should_verify(block_number: u64, touched: TouchedSummary) -> bool {
    if block_number.is_multiple_of(VERIFY_SAMPLE_MOD) {
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
    for _ in 0..INIT_MIGRATION_TICKS_PER_CALL {
        if run_migration_tick(state, INIT_MIGRATION_MAX_STEPS) {
            break;
        }
    }
    if state.state_root_meta.get().initialized {
        ensure_node_db_bootstrapped(state);
    }
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
    // build_state_update_journal_full は空Trie基準の再構築差分を返すため、
    // 既存NodeDBが部分的に残っていると refcnt を二重加算してしまう。
    // ルート欠落時はNodeDBを初期化してから再構築する。
    clear_stable_map(&mut state.state_root_node_db);
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
}

fn compute_storage_root_for_address(state: &StableState, addr: [u8; 20]) -> B256 {
    let lower = make_storage_key(addr, [0u8; 32]);
    let upper = make_storage_key(addr, [0xffu8; 32]);
    let mut slots = Vec::new();
    for entry in state.storage.range(lower..=upper) {
        let key = entry.key().0;
        let Some((key_addr, slot)) = parse_storage_key_bytes(&key) else {
            break;
        };
        if key_addr != addr {
            break;
        }
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
        let Some(addr) = parse_account_key_bytes(&key) else {
            continue;
        };
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
        let Some(addr) = parse_account_key_bytes(&key) else {
            continue;
        };
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
#[path = "state_root_tests.rs"]
mod tests;
