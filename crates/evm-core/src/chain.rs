//! どこで: Phase1のチェーン操作 / 何を: submit/produce/execute / なぜ: 同期Tx体験の基盤のため

use crate::base_fee::compute_next_base_fee;
use crate::bytes::address_to_bytes;
use crate::hash;
use crate::revm_exec::{
    commit_state_diff_to_db, compute_effective_gas_price, execute_tx_on, BlockExecContext,
    ExecError, ExecOutcome, ExecPath, OpHaltReason, StateDiff,
};
use crate::state_root::TouchedSummary;
use crate::trie_commit;
use crate::tx_decode::{decode_ic_synthetic_header_trusted_size, decode_tx};
use crate::tx_submit;
use evm_db::chain_data::constants::{
    DROPPED_RING_CAPACITY, DROP_CODE_CALLER_MISSING, DROP_CODE_DECODE, DROP_CODE_EXEC,
    DROP_CODE_INVALID_FEE, DROP_CODE_MISSING, DROP_CODE_REPLACED, DROP_CODE_RESULT_TOO_LARGE,
    MAX_PENDING_GLOBAL, MAX_PENDING_PER_PRINCIPAL, MAX_PENDING_PER_SENDER, MAX_TX_SIZE,
    READY_CANDIDATE_LIMIT,
};
use evm_db::chain_data::{
    BlockData, CallerKey, Head, PendingFeeKey, PruneJournal, PrunePolicy, ReadyKey, ReadySeqKey,
    ReceiptLike, SenderKey, SenderNonceKey, StoredTx, StoredTxBytes, StoredTxError, TxId,
    TxIndexEntry, TxKind, TxLoc, TxLocKind,
};
use evm_db::meta::tx_locs_v3_active;
use evm_db::stable_state::{
    clear_map as clear_stable_map, with_state, with_state_mut, StableState,
};
use evm_db::types::keys::make_account_key;
use evm_db::types::values::AccountVal;
use evm_db::Storable;
use revm::database::CacheDB;
use revm::context_interface::transaction::{AccessList, AccessListItem};
use revm::database_interface::DatabaseCommit;
use revm::primitives::Address;
use revm::primitives::B256;
use revm::primitives::Bytes as RevmBytes;
use revm::primitives::TxKind as RevmTxKind;
use revm::primitives::U256;
#[cfg(not(target_arch = "wasm32"))]
use std::cell::Cell;
use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap};

const OPS_WARN_RATE_LIMIT_SECS: u64 = 60;
#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static STORE_FAIL_AT_OP: Cell<u64> = const { Cell::new(0) };
    static STORE_OP_COUNTER: Cell<u64> = const { Cell::new(0) };
}

fn current_instruction_counter() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        return ic_cdk::api::performance_counter(
            ic_cdk::api::PerformanceCounterType::InstructionCounter,
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0
    }
}

fn format_tx_id_hex(tx_id: Option<TxId>) -> String {
    match tx_id {
        Some(value) => hex::encode(value.0),
        None => "-".to_string(),
    }
}

pub(crate) fn trap_store_err(
    ctx: &'static str,
    block_number: Option<u64>,
    tx_id: Option<TxId>,
    err_label: &'static str,
) -> ! {
    let block_value = block_number.unwrap_or(0);
    let tx_id_hex = format_tx_id_hex(tx_id);
    ic_cdk::println!(
        "level=error event=storage_write_failed ctx={} block_number={} tx_id={} err={}",
        ctx,
        block_value,
        tx_id_hex,
        err_label
    );
    ic_cdk::trap(&format!(
        "storage.write_failed ctx={} block_number={} tx_id={} err={}",
        ctx, block_value, tx_id_hex, err_label
    ));
}

pub(crate) fn before_store_write_for_test(
    ctx: &'static str,
    block_number: Option<u64>,
    tx_id: Option<TxId>,
) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let op = STORE_OP_COUNTER.with(|counter| {
            let next = counter.get().saturating_add(1);
            counter.set(next);
            next
        });
        let fail_at = STORE_FAIL_AT_OP.with(|value| value.get());
        if fail_at != 0 && op == fail_at {
            trap_store_err(ctx, block_number, tx_id, "injected");
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn configure_store_failpoint_for_test(fail_at_op: Option<u64>) {
    let configured = fail_at_op.unwrap_or(0);
    STORE_FAIL_AT_OP.with(|value| value.set(configured));
    STORE_OP_COUNTER.with(|counter| counter.set(0));
}

fn should_stop_block_execution(
    block_gas_used: u64,
    block_gas_limit: u64,
    instruction_soft_limit: u64,
    instruction_start: u64,
    instruction_current: u64,
) -> bool {
    if block_gas_limit > 0 && block_gas_used >= block_gas_limit {
        return true;
    }
    if instruction_soft_limit == 0 {
        return false;
    }
    instruction_current.saturating_sub(instruction_start) >= instruction_soft_limit
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChainError {
    TxAlreadySeen,
    QueueEmpty,
    TxTooLarge,
    InvalidLimit,
    UnsupportedTxKind,
    DecodeFailed,
    InvalidFee,
    NonceTooLow,
    NonceGap,
    NonceConflict,
    QueueFull,
    SenderQueueFull,
    PrincipalQueueFull,
    ExecFailed(Option<ExecError>),
    InvariantViolation(String),
    NoExecutableTx,
    MintOverflow,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecResult {
    pub tx_id: TxId,
    pub block_number: u64,
    pub tx_index: u32,
    pub status: u8,
    pub gas_used: u64,
    pub return_data: Vec<u8>,
    pub final_status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TxIn {
    EthSigned {
        tx_bytes: Vec<u8>,
        caller_principal: Vec<u8>,
    },
    IcSynthetic {
        caller_principal: Vec<u8>,
        canister_id: Vec<u8>,
        tx_bytes: Vec<u8>,
    },
}

pub fn submit_tx_in(tx_in: TxIn) -> Result<TxId, ChainError> {
    match tx_in {
        TxIn::EthSigned {
            tx_bytes,
            caller_principal,
        } => submit_tx(TxKind::EthSigned, tx_bytes, caller_principal),
        TxIn::IcSynthetic {
            caller_principal,
            canister_id,
            tx_bytes,
        } => submit_ic_tx(caller_principal, canister_id, tx_bytes),
    }
}

pub struct PruneResult {
    pub did_work: bool,
    pub remaining: u64,
    pub pruned_before_block: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallObjectInput {
    pub to: Option<[u8; 20]>,
    pub from: [u8; 20],
    pub gas_limit: Option<u64>,
    pub gas_price: Option<u128>,
    pub nonce: Option<u64>,
    pub max_fee_per_gas: Option<u128>,
    pub max_priority_fee_per_gas: Option<u128>,
    pub chain_id: Option<u64>,
    pub tx_type: Option<u8>,
    pub access_list: Vec<([u8; 20], Vec<[u8; 32]>)>,
    pub value: [u8; 32],
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallObjectResult {
    pub status: u8,
    pub gas_used: u64,
    pub return_data: Vec<u8>,
    pub revert_data: Option<Vec<u8>>,
}

pub struct PruneStatus {
    pub pruning_enabled: bool,
    pub prune_running: bool,
    pub estimated_kept_bytes: u64,
    pub high_water_bytes: u64,
    pub low_water_bytes: u64,
    pub hard_emergency_bytes: u64,
    pub last_prune_at: u64,
    pub pruned_before_block: Option<u64>,
    pub oldest_kept_block: Option<u64>,
    pub oldest_kept_timestamp: Option<u64>,
    pub need_prune: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProduceBlockOutcome {
    pub block: BlockData,
    pub gas_used: u64,
    pub dropped: u64,
}

pub fn set_prune_policy(policy: PrunePolicy) -> Result<(), ChainError> {
    with_state_mut(|state| {
        let mut config = *state.prune_config.get();
        config.set_policy(policy);
        state.prune_config.set(config);
    });
    Ok(())
}

pub fn set_pruning_enabled(enabled: bool) -> Result<(), ChainError> {
    with_state_mut(|state| {
        let mut config = *state.prune_config.get();
        config.pruning_enabled = enabled;
        state.prune_config.set(config);
    });
    Ok(())
}

pub fn state_root_migration_tick(max_steps: u32) -> bool {
    with_state_mut(|state| trie_commit::migration_tick(state, max_steps))
}

pub fn clear_tx_locs_v3() {
    with_state_mut(|state| {
        clear_stable_map(&mut state.tx_locs_v3);
    });
}

pub fn migrate_tx_locs_batch(start_key: Option<TxId>, max_items: u32) -> (Option<TxId>, u64, bool) {
    use std::ops::Bound;
    with_state_mut(|state| {
        let mut copied = 0u64;
        let mut last_key = None;
        let mut iter = match start_key {
            Some(key) => state
                .tx_locs
                .range((Bound::Excluded(key), Bound::Unbounded)),
            None => state.tx_locs.range(..),
        };
        let mut done = false;
        for _ in 0..max_items {
            match iter.next() {
                Some(entry) => {
                    let key = *entry.key();
                    state.tx_locs_v3.insert(key, entry.value());
                    last_key = Some(key);
                    copied = copied.saturating_add(1);
                }
                None => {
                    done = true;
                    break;
                }
            }
        }
        if copied < u64::from(max_items) {
            done = true;
        }
        (last_key, copied, done)
    })
}

pub fn clear_mempool_on_upgrade() {
    with_state_mut(|state| {
        clear_stable_map(&mut state.ready_queue);
        clear_stable_map(&mut state.ready_by_seq);
        clear_stable_map(&mut state.ready_key_by_tx_id);
        clear_stable_map(&mut state.pending_by_sender_nonce);
        clear_stable_map(&mut state.pending_min_nonce);
        clear_stable_map(&mut state.pending_meta_by_tx_id);
        clear_stable_map(&mut state.pending_current_by_sender);
        clear_stable_map(&mut state.principal_pending_count);
        clear_stable_map(&mut state.pending_fee_index);
        clear_stable_map(&mut state.pending_fee_key_by_tx_id);
        clear_stable_map(&mut state.sender_expected_nonce);
    });
}

pub fn rebuild_pending_runtime_indexes() {
    with_state_mut(|state| {
        clear_stable_map(&mut state.principal_pending_count);
        clear_stable_map(&mut state.ready_by_seq);

        let mut pending_ids = Vec::new();
        for entry in state.pending_by_sender_nonce.iter() {
            pending_ids.push(entry.value());
        }
        for tx_id in pending_ids {
            let Some(envelope) = state.tx_store.get(&tx_id) else {
                continue;
            };
            let Ok(stored) = StoredTx::try_from(envelope) else {
                continue;
            };
            let principal = CallerKey::from_principal_bytes(stored.caller_principal.as_slice());
            let count = state.principal_pending_count.get(&principal).unwrap_or(0);
            state
                .principal_pending_count
                .insert(principal, count.saturating_add(1));
        }
        let base_fee = state.chain_state.get().base_fee;
        rebuild_pending_fee_index_for_base_fee(state, base_fee);

        let mut ready_entries = Vec::new();
        for entry in state.ready_key_by_tx_id.iter() {
            ready_entries.push((*entry.key(), entry.value()));
        }
        for (tx_id, ready_key) in ready_entries {
            state
                .ready_by_seq
                .insert(ReadySeqKey::new(ready_key.seq(), tx_id.0), tx_id);
        }
    });
}

fn tx_locs_get(state: &StableState, tx_id: &TxId) -> Option<TxLoc> {
    if tx_locs_v3_active() {
        state.tx_locs_v3.get(tx_id)
    } else {
        state.tx_locs.get(tx_id)
    }
}

fn tx_locs_insert(state: &mut StableState, tx_id: TxId, loc: TxLoc) {
    if tx_locs_v3_active() {
        state.tx_locs_v3.insert(tx_id, loc);
    } else {
        state.tx_locs.insert(tx_id, loc);
    }
}

fn tx_locs_remove(state: &mut StableState, tx_id: &TxId) {
    if tx_locs_v3_active() {
        state.tx_locs_v3.remove(tx_id);
    } else {
        state.tx_locs.remove(tx_id);
    }
}

pub fn get_prune_status() -> PruneStatus {
    with_state(|state| {
        let config = *state.prune_config.get();
        let need_prune = need_prune_internal(state);
        PruneStatus {
            pruning_enabled: config.pruning_enabled,
            prune_running: config.prune_running,
            estimated_kept_bytes: config.estimated_kept_bytes,
            high_water_bytes: config.high_water_bytes,
            low_water_bytes: config.low_water_bytes,
            hard_emergency_bytes: config.hard_emergency_bytes,
            last_prune_at: config.last_prune_at,
            pruned_before_block: state.prune_state.get().pruned_before(),
            oldest_kept_block: config.oldest_block(),
            oldest_kept_timestamp: config.oldest_timestamp(),
            need_prune,
        }
    })
}

pub fn prune_tick() -> Result<PruneResult, ChainError> {
    let should_run = with_state_mut(|state| {
        let mut config = *state.prune_config.get();
        ensure_oldest(state, &mut config);
        if !config.pruning_enabled {
            state.prune_config.set(config);
            return false;
        }
        if config.prune_running {
            state.prune_config.set(config);
            return false;
        }
        state.prune_config.set(config);
        if !need_prune_internal(state) {
            return false;
        }
        let mut config = *state.prune_config.get();
        config.prune_running = true;
        state.prune_config.set(config);
        true
    });
    if !should_run {
        return Ok(PruneResult {
            did_work: false,
            remaining: 0,
            pruned_before_block: with_state(|state| state.prune_state.get().pruned_before()),
        });
    }

    let (retain, max_ops, last_prune_at) = with_state(|state| {
        let policy = state.prune_config.get().policy();
        let retain = compute_retain_count(state, policy);
        (retain, policy.max_ops_per_tick, state.head.get().timestamp)
    });
    let result = prune_blocks(retain, max_ops);
    with_state_mut(|state| {
        let mut config = *state.prune_config.get();
        config.prune_running = false;
        config.last_prune_at = last_prune_at;
        state.prune_config.set(config);
    });
    result
}

fn need_prune_internal(state: &StableState) -> bool {
    let config = *state.prune_config.get();
    let now = state.head.get().timestamp;
    let time_trigger = if config.retain_days > 0 {
        if let Some(oldest_ts) = config.oldest_timestamp() {
            let retain_secs = config.retain_days.saturating_mul(86_400);
            oldest_ts < now.saturating_sub(retain_secs)
        } else {
            false
        }
    } else {
        false
    };
    let cap_trigger =
        config.target_bytes > 0 && config.estimated_kept_bytes > config.high_water_bytes;
    time_trigger || cap_trigger
}

fn compute_retain_count(state: &StableState, policy: PrunePolicy) -> u64 {
    let head = state.head.get().number;
    let config = state.prune_config.get();
    let emergency =
        policy.target_bytes > 0 && config.estimated_kept_bytes > config.hard_emergency_bytes;
    let cap_trigger =
        policy.target_bytes > 0 && config.estimated_kept_bytes > config.high_water_bytes;
    if emergency || cap_trigger {
        // 容量トリガ発動時は retain を無視して古い方から削る
        return 1;
    }
    let mut retain_min_block = 0u64;
    if policy.retain_blocks > 0 {
        let oldest = head.saturating_sub(policy.retain_blocks.saturating_sub(1));
        if oldest > retain_min_block {
            retain_min_block = oldest;
        }
    }
    if policy.retain_days > 0 {
        let retain_secs = policy.retain_days.saturating_mul(86_400);
        let cutoff = state.head.get().timestamp.saturating_sub(retain_secs);
        if let Some((block, _)) = find_block_at_timestamp(state, cutoff) {
            if block > retain_min_block {
                retain_min_block = block;
            }
        }
    }
    let retain = head.saturating_sub(retain_min_block).saturating_add(1);
    if retain == 0 {
        1
    } else {
        retain
    }
}

fn find_block_at_timestamp(state: &StableState, cutoff_ts: u64) -> Option<(u64, u64)> {
    let head = state.head.get().number;
    let mut low = state.prune_config.get().oldest_block().unwrap_or(0);
    let mut high = head;
    let mut best: Option<(u64, u64)> = None;
    while low <= high {
        let mid = low + ((high - low) / 2);
        if let Some(block) = load_block(state, mid) {
            if block.timestamp <= cutoff_ts {
                best = Some((mid, block.timestamp));
                low = mid.saturating_add(1);
            } else if mid == 0 {
                break;
            } else {
                high = mid.saturating_sub(1);
            }
        } else {
            break;
        }
    }
    best
}

pub fn submit_tx(
    kind: TxKind,
    tx_bytes: Vec<u8>,
    caller_principal: Vec<u8>,
) -> Result<TxId, ChainError> {
    let tx_id = TxId(hash::stored_tx_id(kind, &tx_bytes, None, None, None));
    with_state_mut(|state| {
        if tx_bytes.len() > MAX_TX_SIZE {
            return Err(ChainError::TxTooLarge);
        }
        if state.seen_tx.get(&tx_id).is_some() {
            return Err(ChainError::TxAlreadySeen);
        }
        let tx_env = decode_tx(kind, Address::from([0u8; 20]), &tx_bytes)
            .map_err(|_| ChainError::DecodeFailed)?;
        let (max_fee_per_gas, max_priority_fee_per_gas, is_dynamic_fee) =
            fee_fields_from_tx_env(&tx_env);
        let caller_evm = None;
        let envelope = StoredTxBytes::new_with_fees(
            tx_id,
            kind,
            tx_bytes,
            caller_evm,
            Vec::new(),
            caller_principal.clone(),
            max_fee_per_gas,
            max_priority_fee_per_gas,
            is_dynamic_fee,
        );
        let chain_state = state.chain_state.get();
        let base_fee = chain_state.base_fee;
        let min_gas_price = chain_state.min_gas_price;
        let min_priority_fee = chain_state.min_priority_fee;
        if !min_fee_satisfied(&tx_env, base_fee, min_priority_fee, min_gas_price) {
            return Err(ChainError::InvalidFee);
        }
        let effective_gas_price = compute_effective_gas_price(
            max_fee_per_gas,
            if is_dynamic_fee {
                max_priority_fee_per_gas
            } else {
                0
            },
            base_fee,
        )
        .ok_or(ChainError::InvalidFee)?;
        let sender_key = SenderKey::new(address_to_bytes(tx_env.caller));
        let replaced = apply_nonce_and_replacement(
            state,
            sender_key,
            tx_env.nonce,
            effective_gas_price,
            base_fee,
        )?;
        if replaced.is_none() {
            enforce_pending_caps(state, sender_key, &caller_principal, effective_gas_price)?;
        }
        state.seen_tx.insert(tx_id, 1);
        state.tx_store.insert(tx_id, envelope);
        state.pending_current_by_sender.insert(sender_key, tx_id);
        let mut metrics = *state.metrics_state.get();
        metrics.record_submission(1);
        state.metrics_state.set(metrics);
        let mut meta = *state.queue_meta.get();
        let seq = meta.push();
        state.queue_meta.set(meta);
        tx_locs_insert(state, tx_id, TxLoc::queued(seq));
        let mut chain_state = *state.chain_state.get();
        chain_state.next_queue_seq = meta.tail;
        state.chain_state.set(chain_state);
        let pending_key = SenderNonceKey::new(sender_key.0, tx_env.nonce);
        if state.pending_by_sender_nonce.get(&pending_key).is_some() {
            return Err(ChainError::NonceConflict);
        }
        state.pending_by_sender_nonce.insert(pending_key, tx_id);
        state.pending_meta_by_tx_id.insert(tx_id, pending_key);
        track_pending_indexes_on_insert(state, tx_id, &caller_principal, effective_gas_price);
        promote_if_next_nonce(
            state,
            sender_key,
            tx_id,
            tx_env.nonce,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            is_dynamic_fee,
            seq,
        )?;
        Ok(tx_id)
    })
}

pub fn submit_ic_tx(
    caller_principal: Vec<u8>,
    canister_id: Vec<u8>,
    tx_bytes: Vec<u8>,
) -> Result<TxId, ChainError> {
    with_state_mut(|state| {
        if tx_bytes.len() > MAX_TX_SIZE {
            return Err(ChainError::TxTooLarge);
        }
        let caller_evm = hash::caller_evm_from_principal(&caller_principal);
        let sender_key = SenderKey::new(caller_evm);
        let tx_id = TxId(hash::stored_tx_id(
            TxKind::IcSynthetic,
            &tx_bytes,
            Some(caller_evm),
            Some(&canister_id),
            Some(&caller_principal),
        ));
        if state.seen_tx.get(&tx_id).is_some() {
            return Err(ChainError::TxAlreadySeen);
        }
        let header = decode_ic_synthetic_header_trusted_size(&tx_bytes)
            .map_err(|_| ChainError::DecodeFailed)?;
        let nonce = header.nonce;
        let max_fee_per_gas = header.max_fee;
        let max_priority_fee_per_gas = header.max_priority;
        let is_dynamic_fee = true;
        let envelope = StoredTxBytes::new_with_fees(
            tx_id,
            TxKind::IcSynthetic,
            tx_bytes,
            Some(caller_evm),
            canister_id,
            caller_principal.clone(),
            max_fee_per_gas,
            max_priority_fee_per_gas,
            is_dynamic_fee,
        );
        let chain_state = state.chain_state.get();
        let base_fee = chain_state.base_fee;
        let min_gas_price = chain_state.min_gas_price;
        let min_priority_fee = chain_state.min_priority_fee;
        if !min_fee_satisfied_fields(
            max_fee_per_gas,
            Some(max_priority_fee_per_gas),
            base_fee,
            min_priority_fee,
            min_gas_price,
        ) {
            return Err(ChainError::InvalidFee);
        }
        let effective_gas_price = compute_effective_gas_price(
            max_fee_per_gas,
            if is_dynamic_fee {
                max_priority_fee_per_gas
            } else {
                0
            },
            base_fee,
        )
        .ok_or(ChainError::InvalidFee)?;
        let replaced =
            apply_nonce_and_replacement(state, sender_key, nonce, effective_gas_price, base_fee)?;
        if replaced.is_none() {
            enforce_pending_caps(
                state,
                sender_key,
                caller_principal.as_slice(),
                effective_gas_price,
            )?;
        }
        state.seen_tx.insert(tx_id, 1);
        state.tx_store.insert(tx_id, envelope);
        state.pending_current_by_sender.insert(sender_key, tx_id);
        let mut metrics = *state.metrics_state.get();
        metrics.record_submission(1);
        state.metrics_state.set(metrics);
        let mut meta = *state.queue_meta.get();
        let seq = meta.push();
        state.queue_meta.set(meta);
        tx_locs_insert(state, tx_id, TxLoc::queued(seq));
        let mut chain_state = *state.chain_state.get();
        chain_state.next_queue_seq = meta.tail;
        state.chain_state.set(chain_state);
        let pending_key = SenderNonceKey::new(sender_key.0, nonce);
        if state.pending_by_sender_nonce.get(&pending_key).is_some() {
            return Err(ChainError::NonceConflict);
        }
        state.pending_by_sender_nonce.insert(pending_key, tx_id);
        state.pending_meta_by_tx_id.insert(tx_id, pending_key);
        track_pending_indexes_on_insert(state, tx_id, &caller_principal, effective_gas_price);
        promote_if_next_nonce(
            state,
            sender_key,
            tx_id,
            nonce,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            is_dynamic_fee,
            seq,
        )?;
        Ok(tx_id)
    })
}

pub fn credit_balance(address: [u8; 20], amount: u128) -> Result<(), ChainError> {
    let key = make_account_key(address);
    with_state_mut(|state| {
        let existing = state.accounts.get(&key);
        let (nonce, balance, code_hash) = match existing {
            Some(value) => (value.nonce(), value.balance(), value.code_hash()),
            None => (0u64, [0u8; 32], [0u8; 32]),
        };
        let current = U256::from_be_bytes(balance);
        let add = U256::from(amount);
        let next = current.checked_add(add).ok_or(ChainError::MintOverflow)?;
        let updated = AccountVal::from_parts(nonce, next.to_be_bytes(), code_hash);
        state.accounts.insert(key, updated);
        Ok(())
    })
}

pub fn expected_nonce_for_sender_view(sender: [u8; 20]) -> u64 {
    with_state(|state| {
        let sender_key = SenderKey::new(sender);
        if let Some(value) = state.sender_expected_nonce.get(&sender_key) {
            return value;
        }
        let key = make_account_key(sender);
        state
            .accounts
            .get(&key)
            .map(|value| value.nonce())
            .unwrap_or(0)
    })
}

pub fn produce_block(max_txs: usize) -> Result<ProduceBlockOutcome, ChainError> {
    if max_txs == 0 {
        return Err(ChainError::InvalidLimit);
    }
    let head = with_state(|state| *state.head.get());
    let number = head.number.saturating_add(1);
    let timestamp = std::cmp::max(head.timestamp.saturating_add(1), crate::time::now_sec());
    let parent_hash = head.block_hash;
    let exec_ctx = with_state(|state| BlockExecContext {
        block_number: number,
        timestamp,
        base_fee: state.chain_state.get().base_fee,
        block_gas_limit: state.chain_state.get().block_gas_limit,
    });
    let mut included_tx_ids: Vec<TxId> = Vec::new();
    let mut dropped_total = 0u64;
    let mut dropped_by_code = [0u64; evm_db::chain_data::metrics::DROP_CODE_SLOTS];
    let (min_priority_fee, min_gas_price, instruction_soft_limit) = with_state(|state| {
        let chain_state = state.chain_state.get();
        (
            chain_state.min_priority_fee,
            chain_state.min_gas_price,
            chain_state.instruction_soft_limit,
        )
    });
    let mut tx_ids = Vec::new();
    let mut prepared = Vec::new();
    let mut touched_addrs: BTreeSet<[u8; 20]> = BTreeSet::new();
    let mut touched_slots: u32 = 0;
    let mut delta_digests: Vec<[u8; 32]> = Vec::new();
    let mut staged_state_diffs: Vec<StateDiff> = Vec::new();
    let mut staged_drops: Vec<QueuedDrop> = Vec::new();
    let mut staged_included: Vec<StagedIncludedTx> = Vec::new();
    let mut staged_txs: Vec<PreparedTx> = Vec::new();
    with_state(|state| {
        tx_ids = select_ready_candidates(state, state.chain_state.get().base_fee, max_txs);
    });
    if tx_ids.is_empty() {
        return Err(ChainError::QueueEmpty);
    }
    let mut block_gas_used = 0u64;
    let instruction_start = current_instruction_counter();
    for tx_id in tx_ids {
        if should_stop_block_execution(
            block_gas_used,
            exec_ctx.block_gas_limit,
            instruction_soft_limit,
            instruction_start,
            current_instruction_counter(),
        ) {
            break;
        }
        let envelope = with_state(|state| state.tx_store.get(&tx_id));
        let envelope = match envelope {
            Some(value) => value,
            None => {
                prepared.push(PreparedItem::Drop(QueuedDrop {
                    tx_id,
                    drop_code: DROP_CODE_MISSING,
                    sender_override: None,
                    nonce_override: None,
                }));
                continue;
            }
        };
        let stored = match StoredTx::try_from(envelope) {
            Ok(value) => value,
            Err(err) => {
                let drop_code = match err {
                    StoredTxError::MissingCaller => DROP_CODE_CALLER_MISSING,
                    _ => DROP_CODE_DECODE,
                };
                prepared.push(PreparedItem::Drop(QueuedDrop {
                    tx_id,
                    drop_code,
                    sender_override: None,
                    nonce_override: None,
                }));
                continue;
            }
        };
        let kind = stored.kind;
        let caller = match kind {
            TxKind::IcSynthetic => match stored.caller_evm {
                Some(value) => value,
                None => {
                    prepared.push(PreparedItem::Drop(QueuedDrop {
                        tx_id,
                        drop_code: DROP_CODE_CALLER_MISSING,
                        sender_override: None,
                        nonce_override: None,
                    }));
                    continue;
                }
            },
            TxKind::EthSigned => [0u8; 20],
        };
        let tx_env = match decode_tx(kind, Address::from(caller), &stored.raw) {
            Ok(value) => value,
            Err(_) => {
                prepared.push(PreparedItem::Drop(QueuedDrop {
                    tx_id,
                    drop_code: DROP_CODE_DECODE,
                    sender_override: Some(caller),
                    nonce_override: None,
                }));
                continue;
            }
        };
        if !min_fee_satisfied(&tx_env, exec_ctx.base_fee, min_priority_fee, min_gas_price) {
            let sender_bytes = address_to_bytes(tx_env.caller);
            prepared.push(PreparedItem::Drop(QueuedDrop {
                tx_id,
                drop_code: DROP_CODE_INVALID_FEE,
                sender_override: Some(sender_bytes),
                nonce_override: Some(tx_env.nonce),
            }));
            continue;
        }
        let effective = compute_effective_gas_price(
            tx_env.gas_price,
            tx_env.gas_priority_fee.unwrap_or(0),
            exec_ctx.base_fee,
        );
        if effective.is_none() {
            let sender_bytes = address_to_bytes(tx_env.caller);
            prepared.push(PreparedItem::Drop(QueuedDrop {
                tx_id,
                drop_code: DROP_CODE_INVALID_FEE,
                sender_override: Some(sender_bytes),
                nonce_override: Some(tx_env.nonce),
            }));
            continue;
        }
        let sender_bytes = address_to_bytes(tx_env.caller);
        let sender_nonce = tx_env.nonce;
        prepared.push(PreparedItem::Tx(PreparedTx {
            tx_id,
            tx_env,
            sender_bytes,
            sender_nonce,
        }));
        if should_stop_block_execution(
            block_gas_used,
            exec_ctx.block_gas_limit,
            instruction_soft_limit,
            instruction_start,
            current_instruction_counter(),
        ) {
            break;
        }
    }

    for item in prepared {
        match item {
            PreparedItem::Drop(drop) => {
                staged_drops.push(drop);
                track_drop(&mut dropped_total, &mut dropped_by_code, drop.drop_code);
            }
            PreparedItem::Tx(value) => staged_txs.push(value),
        }
    }

    fn apply_drops_only(
        drops: &[QueuedDrop],
        dropped_by_code: &[u64; evm_db::chain_data::metrics::DROP_CODE_SLOTS],
    ) {
        with_state_mut(|state| {
            for drop in drops.iter() {
                advance_sender_after_tx(
                    state,
                    drop.tx_id,
                    drop.sender_override,
                    drop.nonce_override,
                );
                mark_dropped_and_purge_payload(state, drop.tx_id, drop.drop_code);
            }
            let mut metrics = *state.metrics_state.get();
            for (idx, count) in dropped_by_code.iter().enumerate() {
                if *count > 0 {
                    metrics.record_drop(idx as u16, *count);
                }
            }
            state.metrics_state.set(metrics);
        });
    }

    if staged_txs.is_empty() {
        apply_drops_only(&staged_drops, &dropped_by_code);
        return Err(ChainError::NoExecutableTx);
    }

    let mut exec_db = CacheDB::new(crate::revm_db::RevmStableDb);
    for prepared_tx in staged_txs {
        if should_stop_block_execution(
            block_gas_used,
            exec_ctx.block_gas_limit,
            instruction_soft_limit,
            instruction_start,
            current_instruction_counter(),
        ) {
            break;
        }
        let tx_index = u32::try_from(included_tx_ids.len()).unwrap_or(u32::MAX);
        let tx_id = prepared_tx.tx_id;
        let execution = execute_tx_on(
            &mut exec_db,
            tx_id,
            tx_index,
            prepared_tx.tx_env,
            &exec_ctx,
            ExecPath::UserTx,
            false,
        );
        let outcome = match execution {
            Ok((value, user_diff)) => {
                collect_touched_addresses(
                    &user_diff,
                    &mut touched_addrs,
                    &mut touched_slots,
                    &mut delta_digests,
                );
                exec_db.commit(user_diff.clone());
                staged_state_diffs.push(user_diff);
                value
            }
            Err(err) => {
                observe_exec_error(&err, timestamp);
                if err == ExecError::InvalidGasFee {
                    staged_drops.push(QueuedDrop {
                        tx_id,
                        drop_code: DROP_CODE_INVALID_FEE,
                        sender_override: Some(prepared_tx.sender_bytes),
                        nonce_override: Some(prepared_tx.sender_nonce),
                    });
                    track_drop(
                        &mut dropped_total,
                        &mut dropped_by_code,
                        DROP_CODE_INVALID_FEE,
                    );
                    continue;
                }
                if err == ExecError::ResultTooLarge {
                    staged_drops.push(QueuedDrop {
                        tx_id,
                        drop_code: DROP_CODE_RESULT_TOO_LARGE,
                        sender_override: Some(prepared_tx.sender_bytes),
                        nonce_override: Some(prepared_tx.sender_nonce),
                    });
                    track_drop(
                        &mut dropped_total,
                        &mut dropped_by_code,
                        DROP_CODE_RESULT_TOO_LARGE,
                    );
                    continue;
                }
                let output = Vec::new();
                let receipt = ReceiptLike {
                    tx_id,
                    block_number: number,
                    tx_index,
                    status: 0,
                    gas_used: 0,
                    effective_gas_price: 0,
                    l1_data_fee: 0,
                    operator_fee: 0,
                    total_fee: 0,
                    return_data_hash: hash::keccak256(&output),
                    return_data: output,
                    contract_address: None,
                    logs: Vec::new(),
                };
                staged_included.push(StagedIncludedTx::Failed {
                    tx_id,
                    tx_index,
                    receipt,
                    sender_bytes: prepared_tx.sender_bytes,
                    sender_nonce: prepared_tx.sender_nonce,
                });
                included_tx_ids.push(tx_id);
                continue;
            }
        };
        let gas_used = outcome.receipt.gas_used;
        observe_exec_outcome(timestamp, &outcome);
        block_gas_used = block_gas_used.saturating_add(gas_used);
        staged_included.push(StagedIncludedTx::Success {
            tx_id,
            outcome,
            sender_bytes: prepared_tx.sender_bytes,
            sender_nonce: prepared_tx.sender_nonce,
        });
        included_tx_ids.push(tx_id);
        if should_stop_block_execution(
            block_gas_used,
            exec_ctx.block_gas_limit,
            instruction_soft_limit,
            instruction_start,
            current_instruction_counter(),
        ) {
            break;
        }
    }

    if included_tx_ids.is_empty() {
        apply_drops_only(&staged_drops, &dropped_by_code);
        return Err(ChainError::NoExecutableTx);
    }

    let mut tx_id_bytes = Vec::with_capacity(included_tx_ids.len());
    for tx_id in included_tx_ids.iter() {
        tx_id_bytes.push(tx_id.0);
    }
    let tx_list_hash = hash::tx_list_hash(&tx_id_bytes);
    let touched: Vec<[u8; 20]> = touched_addrs.into_iter().collect();
    let summary = TouchedSummary {
        accounts_count: u32::try_from(touched.len()).unwrap_or(u32::MAX),
        slots_count: touched_slots,
        delta_digest: hash::keccak256_concat_chunks(&delta_digests),
    };
    let prepared_root = match with_state_mut(|state| {
        trie_commit::prepare(
            state,
            &staged_state_diffs,
            &touched,
            summary,
            number,
            parent_hash,
            timestamp,
        )
    }) {
        Ok(value) => value,
        Err(reason) => return Err(ChainError::InvariantViolation(reason.to_string())),
    };

    for state_diff in staged_state_diffs {
        commit_state_diff_to_db(state_diff);
    }

    let state_root = prepared_root.state_root;
    let block_hash = hash::block_hash(parent_hash, number, timestamp, tx_list_hash, state_root);
    let block = BlockData::new(
        number,
        parent_hash,
        block_hash,
        timestamp,
        included_tx_ids,
        tx_list_hash,
        state_root,
    );

    with_state_mut(|state| {
        struct StagedPersist {
            tx_id: TxId,
            tx_index: u32,
            tx_index_ptr: evm_db::blob_ptr::BlobPtr,
            receipt_ptr: evm_db::blob_ptr::BlobPtr,
            sender_bytes: [u8; 20],
            sender_nonce: u64,
        }

        trie_commit::apply(state, prepared_root);
        for drop in staged_drops.iter() {
            advance_sender_after_tx(state, drop.tx_id, drop.sender_override, drop.nonce_override);
            mark_dropped_and_purge_payload(state, drop.tx_id, drop.drop_code);
        }
        let mut staged_persisted = Vec::with_capacity(staged_included.len());
        for included in staged_included.iter() {
            match included {
                StagedIncludedTx::Success {
                    tx_id,
                    outcome,
                    sender_bytes,
                    sender_nonce,
                } => {
                    let tx_index_ptr = store_tx_index_entry(
                        state,
                        TxIndexEntry {
                            block_number: number,
                            tx_index: outcome.tx_index,
                        },
                    );
                    let receipt_ptr = store_receipt(state, &outcome.receipt);
                    staged_persisted.push(StagedPersist {
                        tx_id: *tx_id,
                        tx_index: outcome.tx_index,
                        tx_index_ptr,
                        receipt_ptr,
                        sender_bytes: *sender_bytes,
                        sender_nonce: *sender_nonce,
                    });
                }
                StagedIncludedTx::Failed {
                    tx_id,
                    tx_index,
                    receipt,
                    sender_bytes,
                    sender_nonce,
                } => {
                    let tx_index_ptr = store_tx_index_entry(
                        state,
                        TxIndexEntry {
                            block_number: number,
                            tx_index: *tx_index,
                        },
                    );
                    let receipt_ptr = store_receipt(state, receipt);
                    staged_persisted.push(StagedPersist {
                        tx_id: *tx_id,
                        tx_index: *tx_index,
                        tx_index_ptr,
                        receipt_ptr,
                        sender_bytes: *sender_bytes,
                        sender_nonce: *sender_nonce,
                    });
                }
            }
        }

        let block_ptr = store_block(state, &block);
        for persisted in staged_persisted {
            state.tx_index.insert(persisted.tx_id, persisted.tx_index_ptr);
            state.receipts.insert(persisted.tx_id, persisted.receipt_ptr);
            tx_locs_insert(
                state,
                persisted.tx_id,
                TxLoc::included(number, persisted.tx_index),
            );
            advance_sender_after_tx(
                state,
                persisted.tx_id,
                Some(persisted.sender_bytes),
                Some(persisted.sender_nonce),
            );
        }
        state.blocks.insert(number, block_ptr);
        state.head.set(Head {
            number,
            block_hash,
            timestamp,
        });
        let mut chain_state = *state.chain_state.get();
        chain_state.last_block_number = number;
        chain_state.last_block_time = timestamp;
        chain_state.base_fee = compute_next_base_fee(
            chain_state.base_fee,
            block_gas_used,
            chain_state.block_gas_limit,
        );
        state.chain_state.set(chain_state);
        rebuild_pending_fee_index_for_base_fee(state, chain_state.base_fee);
        let mut metrics = *state.metrics_state.get();
        for (idx, count) in dropped_by_code.iter().enumerate() {
            if *count > 0 {
                metrics.record_drop(idx as u16, *count);
            }
        }
        metrics.record_included(block.tx_ids.len() as u64);
        metrics.record_block(number, timestamp, block.tx_ids.len() as u64, dropped_total);
        state.metrics_state.set(metrics);
    });

    Ok(ProduceBlockOutcome {
        block,
        gas_used: block_gas_used,
        dropped: dropped_total,
    })
}

enum StagedIncludedTx {
    Success {
        tx_id: TxId,
        outcome: ExecOutcome,
        sender_bytes: [u8; 20],
        sender_nonce: u64,
    },
    Failed {
        tx_id: TxId,
        tx_index: u32,
        receipt: ReceiptLike,
        sender_bytes: [u8; 20],
        sender_nonce: u64,
    },
}

struct PreparedTx {
    tx_id: TxId,
    tx_env: revm::context::TxEnv,
    sender_bytes: [u8; 20],
    sender_nonce: u64,
}

enum PreparedItem {
    Drop(QueuedDrop),
    Tx(PreparedTx),
}

#[derive(Clone, Copy)]
struct QueuedDrop {
    tx_id: TxId,
    drop_code: u16,
    sender_override: Option<[u8; 20]>,
    nonce_override: Option<u64>,
}

pub fn execute_ic_tx(
    caller_principal: Vec<u8>,
    canister_id: Vec<u8>,
    tx_bytes: Vec<u8>,
) -> Result<ExecResult, ChainError> {
    let caller_evm = hash::caller_evm_from_principal(&caller_principal);
    let tx_id = submit_tx_in(TxIn::IcSynthetic {
        caller_principal,
        canister_id,
        tx_bytes,
    })?;
    execute_and_seal_with_caller(tx_id, TxKind::IcSynthetic, caller_evm)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn execute_submitted_ic_tx_for_test(
    tx_id: TxId,
    caller: [u8; 20],
) -> Result<ExecResult, ChainError> {
    execute_and_seal_with_caller(tx_id, TxKind::IcSynthetic, caller)
}

pub fn eth_call(raw_tx: Vec<u8>) -> Result<Vec<u8>, ChainError> {
    if raw_tx.len() > MAX_TX_SIZE {
        return Err(ChainError::TxTooLarge);
    }
    let tx_env =
        decode_tx(TxKind::EthSigned, Address::ZERO, &raw_tx).map_err(|_| ChainError::DecodeFailed)?;
    let head = with_state(|state| *state.head.get());
    let number = head.number.saturating_add(1);
    let timestamp = std::cmp::max(head.timestamp.saturating_add(1), crate::time::now_sec());
    let exec_ctx = with_state(|state| BlockExecContext {
        block_number: number,
        timestamp,
        base_fee: state.chain_state.get().base_fee,
        block_gas_limit: state.chain_state.get().block_gas_limit,
    });
    let tx_id = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw_tx,
        None,
        None,
        None,
    ));
    let db = CacheDB::new(crate::revm_db::RevmStableDb);
    let (outcome, _) = execute_tx_on(db, tx_id, 0, tx_env, &exec_ctx, ExecPath::UserTx, false)
        .map_err(|err| ChainError::ExecFailed(Some(err)))?;
    Ok(outcome.return_data)
}

pub fn eth_call_object(input: CallObjectInput) -> Result<CallObjectResult, ChainError> {
    if input.data.len() > MAX_TX_SIZE {
        return Err(ChainError::TxTooLarge);
    }
    let head = with_state(|state| *state.head.get());
    let number = head.number.saturating_add(1);
    let timestamp = std::cmp::max(head.timestamp.saturating_add(1), crate::time::now_sec());
    let (base_fee, block_gas_limit) = with_state(|state| {
        let chain = *state.chain_state.get();
        (chain.base_fee, chain.block_gas_limit)
    });
    let gas_limit = input.gas_limit.unwrap_or(block_gas_limit);
    let tx_type = input
        .tx_type
        .unwrap_or(if input.max_fee_per_gas.is_some() || input.max_priority_fee_per_gas.is_some() {
            2
        } else {
            0
        });
    let (gas_price, gas_priority_fee) = if tx_type == 2 {
        (
            input.max_fee_per_gas.unwrap_or(u128::from(base_fee)),
            Some(input.max_priority_fee_per_gas.unwrap_or(0)),
        )
    } else {
        (input.gas_price.unwrap_or(u128::from(base_fee)), None)
    };
    let chain_id = input.chain_id.unwrap_or(evm_db::chain_data::constants::CHAIN_ID);
    let tx_id = call_object_tx_id(&input);
    let access_list = AccessList(
        input
            .access_list
            .into_iter()
            .map(|(address, storage_keys)| AccessListItem {
                address: Address::from(address),
                storage_keys: storage_keys.into_iter().map(B256::from).collect(),
            })
            .collect(),
    );
    let caller = Address::from(input.from);
    let nonce = input.nonce.unwrap_or_else(|| {
        with_state(|state| {
            state
                .accounts
                .get(&make_account_key(input.from))
                .map(|account| account.nonce())
                .unwrap_or(0)
        })
    });
    let tx_env = revm::context::TxEnv {
        caller,
        gas_limit,
        gas_price,
        kind: match input.to {
            Some(to) => RevmTxKind::Call(Address::from(to)),
            None => RevmTxKind::Create,
        },
        value: U256::from_be_bytes(input.value),
        data: RevmBytes::from(input.data.clone()),
        nonce,
        chain_id: Some(chain_id),
        access_list,
        gas_priority_fee,
        blob_hashes: Default::default(),
        max_fee_per_blob_gas: 0,
        authorization_list: Default::default(),
        tx_type,
    };
    let exec_ctx = BlockExecContext {
        block_number: number,
        timestamp,
        base_fee,
        block_gas_limit,
    };
    let db = CacheDB::new(crate::revm_db::RevmStableDb);
    let (outcome, _) = execute_tx_on(db, tx_id, 0, tx_env, &exec_ctx, ExecPath::UserTx, false)
        .map_err(|err| ChainError::ExecFailed(Some(err)))?;
    let revert_data = if outcome.receipt.status == 0 && !outcome.return_data.is_empty() {
        Some(outcome.return_data.clone())
    } else {
        None
    };
    Ok(CallObjectResult {
        status: outcome.receipt.status,
        gas_used: outcome.receipt.gas_used,
        return_data: outcome.return_data,
        revert_data,
    })
}

pub fn eth_estimate_gas_object(input: CallObjectInput) -> Result<u64, ChainError> {
    let out = eth_call_object(input)?;
    Ok(out.gas_used)
}

fn call_object_tx_id(input: &CallObjectInput) -> TxId {
    let mut buf = Vec::with_capacity(1 + 20 + 20 + 8 + 16 + 16 + 8 + 1 + 4 + 32 + 4 + input.data.len());
    buf.extend_from_slice(b"ic-evm:call-object:v1");
    match input.to {
        Some(to) => {
            buf.push(1);
            buf.extend_from_slice(&to);
        }
        None => {
            buf.push(0);
            buf.extend_from_slice(&[0u8; 20]);
        }
    }
    buf.extend_from_slice(&input.from);
    buf.extend_from_slice(&input.gas_limit.unwrap_or(0).to_be_bytes());
    buf.extend_from_slice(&input.gas_price.unwrap_or(0).to_be_bytes());
    buf.extend_from_slice(&input.nonce.unwrap_or(0).to_be_bytes());
    buf.extend_from_slice(&input.max_fee_per_gas.unwrap_or(0).to_be_bytes());
    buf.extend_from_slice(&input.max_priority_fee_per_gas.unwrap_or(0).to_be_bytes());
    buf.extend_from_slice(&input.chain_id.unwrap_or(0).to_be_bytes());
    buf.push(input.tx_type.unwrap_or(0));
    let access_list_len = u32::try_from(input.access_list.len()).unwrap_or(u32::MAX);
    buf.extend_from_slice(&access_list_len.to_be_bytes());
    for (address, storage_keys) in &input.access_list {
        buf.extend_from_slice(address);
        let storage_keys_len = u32::try_from(storage_keys.len()).unwrap_or(u32::MAX);
        buf.extend_from_slice(&storage_keys_len.to_be_bytes());
        for slot in storage_keys {
            buf.extend_from_slice(slot);
        }
    }
    buf.extend_from_slice(&input.value);
    let data_len = u32::try_from(input.data.len()).unwrap_or(u32::MAX);
    buf.extend_from_slice(&data_len.to_be_bytes());
    buf.extend_from_slice(&input.data);
    TxId(hash::keccak256(&buf))
}

pub fn get_block(number: u64) -> Option<BlockData> {
    with_state(|state| load_block(state, number))
}

pub fn get_head_number() -> u64 {
    with_state(|state| state.head.get().number)
}

pub fn get_receipt(tx_id: &TxId) -> Option<ReceiptLike> {
    with_state(|state| load_receipt(state, tx_id))
}

fn store_block(
    state: &mut StableState,
    block: &BlockData,
) -> evm_db::blob_ptr::BlobPtr {
    before_store_write_for_test("store_block", Some(block.number), None);
    let bytes = block.to_bytes().into_owned();
    let ptr = state.blob_store.store_bytes(&bytes).unwrap_or_else(|_| {
        trap_store_err("store_block", Some(block.number), None, "blob_store");
    });
    increment_estimated_kept_bytes(state, ptr.class());
    let mut config = *state.prune_config.get();
    if config.oldest_block().is_none() {
        config.set_oldest(block.number, block.timestamp);
        state.prune_config.set(config);
    }
    ptr
}

fn store_receipt(
    state: &mut StableState,
    receipt: &ReceiptLike,
) -> evm_db::blob_ptr::BlobPtr {
    before_store_write_for_test("store_receipt", Some(receipt.block_number), Some(receipt.tx_id));
    let bytes = receipt.to_bytes().into_owned();
    let ptr = state.blob_store.store_bytes(&bytes).unwrap_or_else(|_| {
        trap_store_err(
            "store_receipt",
            Some(receipt.block_number),
            Some(receipt.tx_id),
            "blob_store",
        );
    });
    increment_estimated_kept_bytes(state, ptr.class());
    ptr
}

fn store_tx_index_entry(
    state: &mut StableState,
    entry: TxIndexEntry,
) -> evm_db::blob_ptr::BlobPtr {
    before_store_write_for_test("store_tx_index_entry", Some(entry.block_number), None);
    let bytes = entry.to_bytes().into_owned();
    let ptr = state.blob_store.store_bytes(&bytes).unwrap_or_else(|_| {
        trap_store_err(
            "store_tx_index_entry",
            Some(entry.block_number),
            None,
            "blob_store",
        );
    });
    increment_estimated_kept_bytes(state, ptr.class());
    ptr
}

fn load_block(state: &StableState, number: u64) -> Option<BlockData> {
    if let Some(ptr) = state.blocks.get(&number) {
        let bytes = state.blob_store.read(&ptr).ok()?;
        return Some(BlockData::from_bytes(Cow::Owned(bytes)));
    }
    None
}

fn load_receipt(state: &StableState, tx_id: &TxId) -> Option<ReceiptLike> {
    if let Some(ptr) = state.receipts.get(tx_id) {
        let bytes = state.blob_store.read(&ptr).ok()?;
        return Some(ReceiptLike::from_bytes(Cow::Owned(bytes)));
    }
    None
}

pub fn get_tx_envelope(tx_id: &TxId) -> Option<StoredTxBytes> {
    with_state(|state| state.tx_store.get(tx_id))
}

fn execute_and_seal_with_caller(
    tx_id: TxId,
    kind: TxKind,
    caller: [u8; 20],
) -> Result<ExecResult, ChainError> {
    let envelope =
        with_state(|state| state.tx_store.get(&tx_id)).ok_or(ChainError::ExecFailed(None))?;
    let mut sync_finalize_guard = SyncFinalizeGuard::new(tx_id);
    let stored = StoredTx::try_from(envelope).map_err(|_| ChainError::DecodeFailed)?;

    let head = with_state(|state| *state.head.get());
    let number = head.number.saturating_add(1);
    let timestamp = head.timestamp.saturating_add(1);
    let parent_hash = head.block_hash;
    let exec_ctx = with_state(|state| BlockExecContext {
        block_number: number,
        timestamp,
        base_fee: state.chain_state.get().base_fee,
        block_gas_limit: state.chain_state.get().block_gas_limit,
    });
    let tx_env = decode_tx(kind, Address::from(caller), &stored.raw)
        .map_err(|_| ChainError::DecodeFailed)?;
    let sender_bytes = address_to_bytes(tx_env.caller);
    let sender_nonce = tx_env.nonce;

    let sync_db = CacheDB::new(crate::revm_db::RevmStableDb);
    let mut staged_state_diffs: Vec<StateDiff> = Vec::new();
    let mut touched_addrs: BTreeSet<[u8; 20]> = BTreeSet::new();
    let mut touched_slots: u32 = 0;
    let mut delta_digests: Vec<[u8; 32]> = Vec::new();
    let (outcome, user_state_diff) = match execute_tx_on(
        sync_db,
        tx_id,
        0,
        tx_env,
        &exec_ctx,
        ExecPath::UserTx,
        false,
    ) {
        Ok(value) => value,
        Err(err) => {
            observe_exec_error(&err, timestamp);
            return Err(ChainError::ExecFailed(Some(err)));
        }
    };
    collect_touched_addresses(
        &user_state_diff,
        &mut touched_addrs,
        &mut touched_slots,
        &mut delta_digests,
    );
    staged_state_diffs.push(user_state_diff);
    observe_exec_outcome(timestamp, &outcome);
    let tx_list_hash = hash::tx_list_hash(&[tx_id.0]);
    let touched: Vec<[u8; 20]> = touched_addrs.into_iter().collect();
    let summary = TouchedSummary {
        accounts_count: u32::try_from(touched.len()).unwrap_or(u32::MAX),
        slots_count: touched_slots,
        delta_digest: hash::keccak256_concat_chunks(&delta_digests),
    };
    let prepared_root = match with_state_mut(|state| {
        trie_commit::prepare(
            state,
            &staged_state_diffs,
            &touched,
            summary,
            number,
            parent_hash,
            timestamp,
        )
    }) {
        Ok(value) => value,
        Err(reason) => {
            sync_finalize_guard.disarm();
            return Err(ChainError::InvariantViolation(reason.to_string()));
        }
    };
    for state_diff in staged_state_diffs {
        commit_state_diff_to_db(state_diff);
    }
    let state_root = prepared_root.state_root;
    let block_hash = hash::block_hash(parent_hash, number, timestamp, tx_list_hash, state_root);

    let block = BlockData::new(
        number,
        parent_hash,
        block_hash,
        timestamp,
        vec![tx_id],
        tx_list_hash,
        state_root,
    );

    with_state_mut(|state| {
        struct StagedSinglePersist {
            block_ptr: evm_db::blob_ptr::BlobPtr,
            tx_index_ptr: evm_db::blob_ptr::BlobPtr,
            receipt_ptr: evm_db::blob_ptr::BlobPtr,
        }

        trie_commit::apply(state, prepared_root);
        let staged = StagedSinglePersist {
            block_ptr: store_block(state, &block),
            tx_index_ptr: store_tx_index_entry(
                state,
                TxIndexEntry {
                    block_number: number,
                    tx_index: outcome.tx_index,
                },
            ),
            receipt_ptr: store_receipt(state, &outcome.receipt),
        };
        state.blocks.insert(number, staged.block_ptr);
        state.tx_index.insert(tx_id, staged.tx_index_ptr);
        state.receipts.insert(tx_id, staged.receipt_ptr);
        tx_locs_insert(state, tx_id, TxLoc::included(number, outcome.tx_index));
        state.head.set(Head {
            number,
            block_hash,
            timestamp,
        });
        advance_sender_after_tx(state, tx_id, Some(sender_bytes), Some(sender_nonce));
        let mut chain_state = *state.chain_state.get();
        chain_state.last_block_number = number;
        chain_state.last_block_time = timestamp;
        chain_state.base_fee = compute_next_base_fee(
            chain_state.base_fee,
            outcome.receipt.gas_used,
            chain_state.block_gas_limit,
        );
        state.chain_state.set(chain_state);
        rebuild_pending_fee_index_for_base_fee(state, chain_state.base_fee);
        let mut metrics = *state.metrics_state.get();
        metrics.record_included(1);
        metrics.record_block(number, timestamp, 1, 0);
        state.metrics_state.set(metrics);
    });
    sync_finalize_guard.disarm();

    Ok(ExecResult {
        tx_id,
        block_number: number,
        tx_index: outcome.tx_index,
        status: outcome.receipt.status,
        gas_used: outcome.receipt.gas_used,
        return_data: outcome.return_data,
        final_status: outcome.final_status,
    })
}

pub fn get_tx_loc(tx_id: &TxId) -> Option<TxLoc> {
    with_state(|state| tx_locs_get(state, tx_id))
}

pub fn prune_blocks(retain: u64, max_ops: u32) -> Result<PruneResult, ChainError> {
    if retain == 0 || max_ops == 0 {
        return Err(ChainError::InvalidLimit);
    }
    with_state_mut(|state| {
        recover_prune_journal(state)?;
        let head_number = state.head.get().number;
        if head_number <= retain {
            let pruned_before = state.prune_state.get().pruned_before();
            return Ok(PruneResult {
                did_work: false,
                remaining: 0,
                pruned_before_block: pruned_before,
            });
        }
        let prune_before = head_number.saturating_sub(retain);
        let mut prune_state = *state.prune_state.get();
        let mut next = prune_state.next_prune_block;
        if let Some(pruned) = prune_state.pruned_before() {
            if next <= pruned {
                next = pruned.saturating_add(1);
            }
        }
        let mut did_work = false;
        let mut ops_used: u64 = 0;
        let max_ops = u64::from(max_ops);
        while next <= prune_before {
            let block = match load_block(state, next) {
                Some(value) => value,
                None => {
                    prune_state.set_pruned_before(next);
                    next = next.saturating_add(1);
                    did_work = true;
                    continue;
                }
            };
            let needed = 1u64 + (block.tx_ids.len() as u64).saturating_mul(5);
            if ops_used.saturating_add(needed) > max_ops {
                break;
            }
            let mut ptrs = collect_ptrs_for_block(state, next, &block);
            for ptr in ptrs.iter() {
                state
                    .blob_store
                    .mark_quarantine(ptr)
                    .map_err(|_| ChainError::ExecFailed(None))?;
            }
            state
                .prune_journal
                .insert(next, PruneJournal { ptrs: ptrs.clone() });
            prune_state.set_journal_block(next);
            // WAL: persist recovery cursor before destructive deletes.
            state.prune_state.set(prune_state);

            let _ = state.blocks.remove(&next);
            for tx_id in block.tx_ids.iter() {
                remove_pending_fee_index_by_tx_id(state, *tx_id);
                decrement_principal_pending_count_for_tx(state, *tx_id);
                state.receipts.remove(tx_id);
                state.tx_index.remove(tx_id);
                tx_locs_remove(state, tx_id);
                state.tx_store.remove(tx_id);
                state.seen_tx.remove(tx_id);
            }
            ops_used = ops_used.saturating_add(needed);
            prune_state.set_pruned_before(next);
            next = next.saturating_add(1);
            did_work = true;

            for ptr in ptrs.drain(..) {
                state
                    .blob_store
                    .mark_free(&ptr)
                    .map_err(|_| ChainError::ExecFailed(None))?;
                decrement_estimated_kept_bytes(state, ptr.class());
            }
            state.prune_journal.remove(&next.saturating_sub(1));
            prune_state.clear_journal();
            state.prune_state.set(prune_state);
        }
        prune_state.next_prune_block = next;
        state.prune_state.set(prune_state);
        refresh_oldest(state);
        let remaining = if next > prune_before {
            0
        } else {
            prune_before.saturating_sub(next).saturating_add(1)
        };
        Ok(PruneResult {
            did_work,
            remaining,
            pruned_before_block: prune_state.pruned_before(),
        })
    })
}

fn recover_prune_journal(state: &mut evm_db::stable_state::StableState) -> Result<(), ChainError> {
    let mut prune_state = *state.prune_state.get();
    let journal_block = match prune_state.journal_block() {
        Some(value) => value,
        None => match state.prune_journal.range(..).next() {
            // Fallback: recover from the oldest orphan journal entry when the cursor is missing.
            Some(entry) => {
                let value = *entry.key();
                prune_state.set_journal_block(value);
                state.prune_state.set(prune_state);
                value
            }
            None => {
                if let Some(pruned) = prune_state.pruned_before() {
                    let min_next = pruned.saturating_add(1);
                    if prune_state.next_prune_block < min_next {
                        prune_state.next_prune_block = min_next;
                        state.prune_state.set(prune_state);
                    }
                }
                return Ok(());
            }
        },
    };
    if let Some(journal) = state.prune_journal.get(&journal_block) {
        if let Some(block) = load_block(state, journal_block) {
            let _ = state.blocks.remove(&journal_block);
            for tx_id in block.tx_ids.iter() {
                remove_pending_fee_index_by_tx_id(state, *tx_id);
                decrement_principal_pending_count_for_tx(state, *tx_id);
                state.receipts.remove(tx_id);
                state.tx_index.remove(tx_id);
                tx_locs_remove(state, tx_id);
                state.tx_store.remove(tx_id);
                state.seen_tx.remove(tx_id);
            }
            if let Some(pruned) = prune_state.pruned_before() {
                if pruned < journal_block {
                    prune_state.set_pruned_before(journal_block);
                }
            } else {
                prune_state.set_pruned_before(journal_block);
            }
        }
        for ptr in journal.ptrs.iter() {
            state
                .blob_store
                .mark_free(ptr)
                .map_err(|_| ChainError::ExecFailed(None))?;
            decrement_estimated_kept_bytes(state, ptr.class());
        }
        state.prune_journal.remove(&journal_block);
    }
    prune_state.clear_journal();
    if let Some(pruned) = prune_state.pruned_before() {
        let min_next = pruned.saturating_add(1);
        if prune_state.next_prune_block < min_next {
            prune_state.next_prune_block = min_next;
        }
    }
    state.prune_state.set(prune_state);
    refresh_oldest(state);
    Ok(())
}

fn collect_ptrs_for_block(
    state: &evm_db::stable_state::StableState,
    block_number: u64,
    block: &BlockData,
) -> Vec<evm_db::blob_ptr::BlobPtr> {
    let mut out = Vec::new();
    if let Some(ptr) = state.blocks.get(&block_number) {
        out.push(ptr);
    }
    for tx_id in block.tx_ids.iter() {
        if let Some(ptr) = state.receipts.get(tx_id) {
            out.push(ptr);
        }
        if let Some(ptr) = state.tx_index.get(tx_id) {
            out.push(ptr);
        }
    }
    out
}

fn increment_estimated_kept_bytes(state: &mut StableState, class: u32) {
    let mut config = *state.prune_config.get();
    config.estimated_kept_bytes = config.estimated_kept_bytes.saturating_add(u64::from(class));
    state.prune_config.set(config);
}

fn decrement_estimated_kept_bytes(state: &mut StableState, class: u32) {
    let mut config = *state.prune_config.get();
    config.estimated_kept_bytes = config.estimated_kept_bytes.saturating_sub(u64::from(class));
    state.prune_config.set(config);
}

fn refresh_oldest(state: &mut StableState) {
    let next = match state.prune_state.get().pruned_before() {
        Some(value) => value.saturating_add(1),
        None => 0,
    };
    if let Some((block_number, timestamp)) = find_next_existing_block(state, next) {
        let mut config = *state.prune_config.get();
        config.set_oldest(block_number, timestamp);
        state.prune_config.set(config);
    }
}

fn ensure_oldest(state: &mut StableState, config: &mut evm_db::chain_data::PruneConfigV1) {
    if config.oldest_block().is_some() {
        return;
    }
    let next = match state.prune_state.get().pruned_before() {
        Some(value) => value.saturating_add(1),
        None => 0,
    };
    if let Some((block_number, timestamp)) = find_next_existing_block(state, next) {
        config.set_oldest(block_number, timestamp);
    }
}

fn find_next_existing_block(state: &StableState, start: u64) -> Option<(u64, u64)> {
    for entry in state.blocks.range(start..) {
        let number = *entry.key();
        if let Some(block) = load_block(state, number) {
            return Some((number, block.timestamp));
        }
    }
    None
}

pub struct QueueItem {
    pub seq: u64,
    pub tx_id: TxId,
    pub kind: TxKind,
}

pub struct QueueSnapshot {
    pub items: Vec<QueueItem>,
    pub next_cursor: Option<u64>,
}

pub fn get_queue_snapshot(limit: usize, cursor: Option<u64>) -> QueueSnapshot {
    with_state(|state| {
        let mut items = Vec::new();
        let mut next_cursor = None;
        let mut last_seq = None;
        let start_seq = cursor.unwrap_or(0);
        let start_key = ReadySeqKey::new(start_seq, [0u8; 32]);
        for entry in state.ready_by_seq.range(start_key..) {
            if items.len() >= limit {
                next_cursor = last_seq.map(|seq: u64| seq.saturating_add(1));
                break;
            }
            let tx_id = entry.value();
            let stored = match state
                .tx_store
                .get(&tx_id)
                .and_then(|e| StoredTx::try_from(e).ok())
            {
                Some(value) => value,
                None => {
                    continue;
                }
            };
            let key = entry.key().0;
            let mut seq = [0u8; 8];
            seq.copy_from_slice(&key[0..8]);
            items.push(QueueItem {
                seq: u64::from_be_bytes(seq),
                tx_id,
                kind: stored.kind,
            });
            last_seq = Some(u64::from_be_bytes(seq));
        }
        QueueSnapshot { items, next_cursor }
    })
}

fn track_drop(total: &mut u64, by_code: &mut [u64], code: u16) {
    *total = total.saturating_add(1);
    let idx = usize::from(code);
    if idx < by_code.len() {
        by_code[idx] = by_code[idx].saturating_add(1);
    }
}

fn observe_exec_error(err: &ExecError, now: u64) {
    if let ExecError::EvmHalt(OpHaltReason::Unknown) = err {
        record_exec_halt_unknown(now);
    }
}

fn observe_exec_outcome(now: u64, outcome: &ExecOutcome) {
    if outcome.halt_reason == Some(OpHaltReason::Unknown) {
        record_exec_halt_unknown(now);
    }
}

fn record_exec_halt_unknown(now: u64) {
    let should_warn = with_state_mut(|state| {
        let mut metrics = *state.ops_metrics.get();
        metrics.exec_halt_unknown_count = metrics.exec_halt_unknown_count.saturating_add(1);
        let should_warn = metrics.last_exec_halt_unknown_warn_ts == 0
            || now.saturating_sub(metrics.last_exec_halt_unknown_warn_ts)
                >= OPS_WARN_RATE_LIMIT_SECS;
        if should_warn {
            metrics.last_exec_halt_unknown_warn_ts = now;
        }
        state.ops_metrics.set(metrics);
        should_warn
    });
    if should_warn {
        eprintln!("exec halt reason fell back to unknown");
    }
}

fn promote_if_next_nonce(
    state: &mut evm_db::stable_state::StableState,
    sender: SenderKey,
    tx_id: TxId,
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
    is_dynamic_fee: bool,
    seq: u64,
) -> Result<(), ChainError> {
    match state.pending_min_nonce.get(&sender) {
        None => {
            state.pending_min_nonce.insert(sender, nonce);
            insert_ready(
                state,
                tx_id,
                max_fee_per_gas,
                max_priority_fee_per_gas,
                is_dynamic_fee,
                seq,
            )?;
        }
        Some(current) => {
            if nonce < current {
                let old_key = SenderNonceKey::new(sender.0, current);
                if let Some(old_tx_id) = state.pending_by_sender_nonce.get(&old_key) {
                    remove_ready_by_tx_id(state, old_tx_id);
                }
                state.pending_min_nonce.insert(sender, nonce);
                insert_ready(
                    state,
                    tx_id,
                    max_fee_per_gas,
                    max_priority_fee_per_gas,
                    is_dynamic_fee,
                    seq,
                )?;
            }
        }
    }
    Ok(())
}

fn enforce_pending_caps(
    state: &mut evm_db::stable_state::StableState,
    sender: SenderKey,
    caller_principal: &[u8],
    incoming_effective_gas_price: u64,
) -> Result<(), ChainError> {
    if count_pending_for_sender(state, sender) >= MAX_PENDING_PER_SENDER {
        return Err(ChainError::SenderQueueFull);
    }
    if count_pending_for_principal(state, caller_principal) >= MAX_PENDING_PER_PRINCIPAL {
        return Err(ChainError::PrincipalQueueFull);
    }
    if state.pending_by_sender_nonce.len() >= MAX_PENDING_GLOBAL as u64 {
        evict_lowest_fee_pending(state, incoming_effective_gas_price)?;
    }
    Ok(())
}

fn count_pending_for_principal(
    state: &evm_db::stable_state::StableState,
    caller_principal: &[u8],
) -> usize {
    if caller_principal.is_empty() {
        return 0;
    }
    let key = CallerKey::from_principal_bytes(caller_principal);
    usize::try_from(state.principal_pending_count.get(&key).unwrap_or(0)).unwrap_or(usize::MAX)
}

fn evict_lowest_fee_pending(
    state: &mut evm_db::stable_state::StableState,
    incoming_effective_gas_price: u64,
) -> Result<(), ChainError> {
    let Some(entry) = state.pending_fee_index.range(..).next() else {
        return Err(ChainError::QueueFull);
    };
    let key = *entry.key();
    let mut fee = [0u8; 8];
    fee.copy_from_slice(&key.0[0..8]);
    let lowest_fee = u64::from_be_bytes(fee);
    let evict_tx_id = entry.value();
    if incoming_effective_gas_price <= lowest_fee {
        return Err(ChainError::QueueFull);
    }

    // Queue pressure eviction is modeled as a replacement-style drop for telemetry consistency.
    advance_sender_after_tx(state, evict_tx_id, None, None);
    mark_dropped_and_purge_payload(state, evict_tx_id, DROP_CODE_REPLACED);
    let mut metrics = *state.metrics_state.get();
    metrics.record_drop(DROP_CODE_REPLACED, 1);
    state.metrics_state.set(metrics);
    Ok(())
}

fn count_pending_for_sender(state: &evm_db::stable_state::StableState, sender: SenderKey) -> usize {
    let mut count = 0usize;
    let start = SenderNonceKey::new(sender.0, 0);
    for entry in state.pending_by_sender_nonce.range(start..) {
        let key = *entry.key();
        if key.sender != sender {
            break;
        }
        count = count.saturating_add(1);
    }
    count
}

fn track_pending_indexes_on_insert(
    state: &mut evm_db::stable_state::StableState,
    tx_id: TxId,
    caller_principal: &[u8],
    effective_gas_price: u64,
) {
    let principal_key = CallerKey::from_principal_bytes(caller_principal);
    let current = state
        .principal_pending_count
        .get(&principal_key)
        .unwrap_or(0);
    state
        .principal_pending_count
        .insert(principal_key, current.saturating_add(1));
    let fee_key = PendingFeeKey::new(effective_gas_price, tx_id.0);
    state.pending_fee_index.insert(fee_key, tx_id);
    state.pending_fee_key_by_tx_id.insert(tx_id, fee_key);
}

fn rebuild_pending_fee_index_for_base_fee(
    state: &mut evm_db::stable_state::StableState,
    base_fee: u64,
) {
    clear_stable_map(&mut state.pending_fee_index);
    clear_stable_map(&mut state.pending_fee_key_by_tx_id);
    let mut pending_ids = Vec::new();
    for entry in state.pending_by_sender_nonce.iter() {
        pending_ids.push(entry.value());
    }
    for tx_id in pending_ids {
        let Some(envelope) = state.tx_store.get(&tx_id) else {
            continue;
        };
        let Ok(stored) = StoredTx::try_from(envelope) else {
            continue;
        };
        let effective = compute_effective_gas_price(
            stored.max_fee_per_gas,
            if stored.is_dynamic_fee {
                stored.max_priority_fee_per_gas
            } else {
                0
            },
            base_fee,
        )
        .unwrap_or(0);
        let fee_key = PendingFeeKey::new(effective, tx_id.0);
        state.pending_fee_index.insert(fee_key, tx_id);
        state.pending_fee_key_by_tx_id.insert(tx_id, fee_key);
    }
}

fn remove_pending_fee_index_by_tx_id(state: &mut evm_db::stable_state::StableState, tx_id: TxId) {
    if let Some(key) = state.pending_fee_key_by_tx_id.remove(&tx_id) {
        state.pending_fee_index.remove(&key);
    }
}

fn decrement_principal_pending_count_for_tx(
    state: &mut evm_db::stable_state::StableState,
    tx_id: TxId,
) {
    let Some(envelope) = state.tx_store.get(&tx_id) else {
        return;
    };
    let Ok(stored) = StoredTx::try_from(envelope) else {
        return;
    };
    let key = CallerKey::from_principal_bytes(stored.caller_principal.as_slice());
    let current = state.principal_pending_count.get(&key).unwrap_or(0);
    if current <= 1 {
        state.principal_pending_count.remove(&key);
    } else {
        state.principal_pending_count.insert(key, current - 1);
    }
}

fn insert_ready(
    state: &mut evm_db::stable_state::StableState,
    tx_id: TxId,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
    is_dynamic_fee: bool,
    seq: u64,
) -> Result<(), ChainError> {
    let priority = if is_dynamic_fee {
        max_priority_fee_per_gas
    } else {
        0
    };
    let key = ReadyKey::new(max_fee_per_gas, priority, seq, tx_id.0);
    state.ready_queue.insert(key, tx_id);
    state
        .ready_by_seq
        .insert(ReadySeqKey::new(seq, tx_id.0), tx_id);
    state.ready_key_by_tx_id.insert(tx_id, key);
    Ok(())
}

fn remove_ready_by_tx_id(state: &mut evm_db::stable_state::StableState, tx_id: TxId) {
    if let Some(key) = state.ready_key_by_tx_id.remove(&tx_id) {
        state.ready_queue.remove(&key);
        state
            .ready_by_seq
            .remove(&ReadySeqKey::new(key.seq(), tx_id.0));
    }
}

fn advance_sender_after_tx(
    state: &mut evm_db::stable_state::StableState,
    tx_id: TxId,
    sender_override: Option<[u8; 20]>,
    nonce_override: Option<u64>,
) {
    remove_ready_by_tx_id(state, tx_id);
    let pending_key = match state.pending_meta_by_tx_id.remove(&tx_id) {
        Some(key) => key,
        None => match (sender_override, nonce_override) {
            (Some(sender), Some(nonce)) => SenderNonceKey::new(sender, nonce),
            _ => return,
        },
    };
    state.pending_by_sender_nonce.remove(&pending_key);
    remove_pending_fee_index_by_tx_id(state, tx_id);
    decrement_principal_pending_count_for_tx(state, tx_id);
    finalize_pending_for_sender(state, pending_key.sender, tx_id);
    let sender = pending_key.sender;
    if state.pending_min_nonce.get(&sender) != Some(pending_key.nonce) {
        return;
    }
    let mut cursor_nonce = pending_key.nonce;
    loop {
        match next_pending_for_sender(state, sender, cursor_nonce) {
            Some((next_nonce, next_tx_id)) => {
                state.pending_min_nonce.insert(sender, next_nonce);
                match load_fee_fields_and_seq(state, next_tx_id) {
                    Ok(Some((max_fee_per_gas, max_priority_fee_per_gas, is_dynamic_fee, seq))) => {
                        let _ = insert_ready(
                            state,
                            next_tx_id,
                            max_fee_per_gas,
                            max_priority_fee_per_gas,
                            is_dynamic_fee,
                            seq,
                        );
                        return;
                    }
                    Ok(None) => {
                        drop_invalid_fee_pending(state, next_tx_id, None, None);
                    }
                    Err(RekeyError::DecodeFailed) => {
                        drop_invalid_fee_pending_decode(state, next_tx_id, None, None);
                    }
                }
                cursor_nonce = next_nonce;
            }
            None => {
                state.pending_min_nonce.remove(&sender);
                return;
            }
        }
    }
}

fn drop_invalid_fee_pending(
    state: &mut evm_db::stable_state::StableState,
    tx_id: TxId,
    dropped_total: Option<&mut u64>,
    dropped_by_code: Option<&mut [u64]>,
) {
    advance_sender_after_tx(state, tx_id, None, None);
    mark_dropped_and_purge_payload(state, tx_id, DROP_CODE_INVALID_FEE);
    if let (Some(total), Some(by_code)) = (dropped_total, dropped_by_code) {
        track_drop(total, by_code, DROP_CODE_INVALID_FEE);
    } else {
        let mut metrics = *state.metrics_state.get();
        metrics.record_drop(DROP_CODE_INVALID_FEE, 1);
        state.metrics_state.set(metrics);
    }
}

fn drop_invalid_fee_pending_decode(
    state: &mut evm_db::stable_state::StableState,
    tx_id: TxId,
    dropped_total: Option<&mut u64>,
    dropped_by_code: Option<&mut [u64]>,
) {
    advance_sender_after_tx(state, tx_id, None, None);
    mark_dropped_and_purge_payload(state, tx_id, DROP_CODE_DECODE);
    if let (Some(total), Some(by_code)) = (dropped_total, dropped_by_code) {
        track_drop(total, by_code, DROP_CODE_DECODE);
    } else {
        let mut metrics = *state.metrics_state.get();
        metrics.record_drop(DROP_CODE_DECODE, 1);
        state.metrics_state.set(metrics);
    }
}

fn drop_exec_pending_sync(state: &mut evm_db::stable_state::StableState, tx_id: TxId) {
    let has_pending = state.pending_meta_by_tx_id.get(&tx_id).is_some();
    let has_ready = state.ready_key_by_tx_id.get(&tx_id).is_some();
    if !has_pending && !has_ready {
        return;
    }
    remove_ready_by_tx_id(state, tx_id);
    remove_pending_fee_index_by_tx_id(state, tx_id);
    decrement_principal_pending_count_for_tx(state, tx_id);
    if let Some(pending_key) = state.pending_meta_by_tx_id.remove(&tx_id) {
        state.pending_by_sender_nonce.remove(&pending_key);
        finalize_pending_for_sender(state, pending_key.sender, tx_id);
    }
    mark_dropped_and_purge_payload(state, tx_id, DROP_CODE_EXEC);
    let mut metrics = *state.metrics_state.get();
    metrics.record_drop(DROP_CODE_EXEC, 1);
    state.metrics_state.set(metrics);
}

struct SyncFinalizeGuard {
    tx_id: TxId,
    active: bool,
}

impl SyncFinalizeGuard {
    fn new(tx_id: TxId) -> Self {
        Self {
            tx_id,
            active: true,
        }
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for SyncFinalizeGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        with_state_mut(|state| drop_exec_pending_sync(state, self.tx_id));
    }
}

fn next_pending_for_sender(
    state: &mut evm_db::stable_state::StableState,
    sender: SenderKey,
    after_nonce: u64,
) -> Option<(u64, TxId)> {
    let start = SenderNonceKey::new(sender.0, after_nonce.saturating_add(1));
    for entry in state.pending_by_sender_nonce.range(start..) {
        let key = *entry.key();
        if key.sender != sender {
            break;
        }
        return Some((key.nonce, entry.value()));
    }
    None
}

fn load_fee_fields_and_seq(
    state: &evm_db::stable_state::StableState,
    tx_id: TxId,
) -> Result<Option<(u128, u128, bool, u64)>, RekeyError> {
    let envelope = match state.tx_store.get(&tx_id) {
        Some(value) => value,
        None => return Ok(None),
    };
    let stored = StoredTx::try_from(envelope).map_err(|_| RekeyError::DecodeFailed)?;
    let (max_fee_per_gas, max_priority_fee_per_gas, is_dynamic_fee) = (
        stored.max_fee_per_gas,
        stored.max_priority_fee_per_gas,
        stored.is_dynamic_fee,
    );
    let seq = match tx_locs_get(state, &tx_id) {
        Some(loc) => loc.seq,
        None => return Ok(None),
    };
    Ok(Some((
        max_fee_per_gas,
        max_priority_fee_per_gas,
        is_dynamic_fee,
        seq,
    )))
}

fn apply_nonce_and_replacement(
    state: &mut evm_db::stable_state::StableState,
    sender: SenderKey,
    nonce: u64,
    effective_gas_price: u64,
    base_fee: u64,
) -> Result<Option<TxId>, ChainError> {
    let replaced = match tx_submit::apply_nonce_and_replacement(
        state,
        sender,
        nonce,
        effective_gas_price,
        base_fee,
    ) {
        Ok(value) => value,
        Err(tx_submit::NonceRuleError::TooLow) => return Err(ChainError::NonceTooLow),
        Err(tx_submit::NonceRuleError::Gap) => return Err(ChainError::NonceGap),
        Err(tx_submit::NonceRuleError::Conflict) => return Err(ChainError::NonceConflict),
    };
    if let Some(old_tx_id) = replaced {
        replace_pending_for_sender(state, sender, old_tx_id);
    }
    Ok(replaced)
}

fn finalize_pending_for_sender(
    state: &mut evm_db::stable_state::StableState,
    sender: SenderKey,
    tx_id: TxId,
) {
    tx_submit::finalize_pending_for_sender(state, sender, tx_id);
}

fn replace_pending_for_sender(
    state: &mut evm_db::stable_state::StableState,
    sender: SenderKey,
    old_tx_id: TxId,
) {
    remove_ready_by_tx_id(state, old_tx_id);
    remove_pending_fee_index_by_tx_id(state, old_tx_id);
    decrement_principal_pending_count_for_tx(state, old_tx_id);
    if let Some(pending_key) = state.pending_meta_by_tx_id.remove(&old_tx_id) {
        state.pending_by_sender_nonce.remove(&pending_key);
    }
    state.pending_min_nonce.remove(&sender);
    state.pending_current_by_sender.remove(&sender);
    mark_dropped_and_purge_payload(state, old_tx_id, DROP_CODE_REPLACED);
    let mut metrics = *state.metrics_state.get();
    metrics.record_drop(DROP_CODE_REPLACED, 1);
    state.metrics_state.set(metrics);
}

fn mark_dropped_and_purge_payload(
    state: &mut evm_db::stable_state::StableState,
    tx_id: TxId,
    drop_code: u16,
) {
    remove_pending_fee_index_by_tx_id(state, tx_id);
    state.tx_store.remove(&tx_id);
    tx_locs_insert(state, tx_id, TxLoc::dropped(drop_code));
    push_dropped_ring(state, tx_id);
}

fn push_dropped_ring(state: &mut evm_db::stable_state::StableState, tx_id: TxId) {
    let mut ring = *state.dropped_ring_state.get();
    let seq = ring.next_seq;
    state.dropped_ring.insert(seq, tx_id);
    ring.next_seq = ring.next_seq.saturating_add(1);
    if u64::from(ring.len) < DROPPED_RING_CAPACITY {
        ring.len = ring.len.saturating_add(1);
        state.dropped_ring_state.set(ring);
        return;
    }

    let evict_seq = seq.saturating_sub(DROPPED_RING_CAPACITY);
    if let Some(evicted_tx_id) = state.dropped_ring.remove(&evict_seq) {
        if let Some(loc) = tx_locs_get(state, &evicted_tx_id) {
            if loc.kind == TxLocKind::Dropped {
                tx_locs_remove(state, &evicted_tx_id);
            }
        }
    }
    state.dropped_ring_state.set(ring);
}

fn collect_touched_addresses(
    state_diff: &StateDiff,
    out: &mut BTreeSet<[u8; 20]>,
    slots_out: &mut u32,
    digest_out: &mut Vec<[u8; 32]>,
) {
    for (address, account) in state_diff.iter() {
        let mut buf = [0u8; 20];
        buf.copy_from_slice(address.as_ref());
        out.insert(buf);
        let mut payload = Vec::new();
        payload.extend_from_slice(address.as_ref());
        let mut slot_count = 0u32;
        for (slot, entry) in account.changed_storage_slots() {
            payload.extend_from_slice(&slot.to_be_bytes::<32>());
            payload.extend_from_slice(&entry.present_value.to_be_bytes::<32>());
            slot_count = slot_count.saturating_add(1);
        }
        *slots_out = slots_out.saturating_add(slot_count);
        digest_out.push(hash::keccak256(&payload));
    }
}

fn min_fee_satisfied(
    tx_env: &revm::context::TxEnv,
    base_fee: u64,
    min_priority_fee: u64,
    min_gas_price: u64,
) -> bool {
    min_fee_satisfied_fields(
        tx_env.gas_price,
        tx_env.gas_priority_fee,
        base_fee,
        min_priority_fee,
        min_gas_price,
    )
}

fn min_fee_satisfied_fields(
    gas_price: u128,
    gas_priority_fee: Option<u128>,
    base_fee: u64,
    min_priority_fee: u64,
    min_gas_price: u64,
) -> bool {
    if let Some(priority) = gas_priority_fee {
        let min_priority_fee = u128::from(min_priority_fee);
        if priority < min_priority_fee {
            return false;
        }
        let base_fee = u128::from(base_fee);
        let base_plus_min = base_fee.saturating_add(min_priority_fee);
        gas_price >= base_fee && gas_price >= base_plus_min
    } else {
        gas_price >= u128::from(min_gas_price)
    }
}

// V2に保存するため、TxEnvから確定済みのfee値を抽出する。
fn fee_fields_from_tx_env(tx_env: &revm::context::TxEnv) -> (u128, u128, bool) {
    let max_fee_per_gas = tx_env.gas_price;
    let max_priority_fee_per_gas = tx_env.gas_priority_fee.unwrap_or(0);
    let is_dynamic_fee = tx_env.gas_priority_fee.is_some();
    (max_fee_per_gas, max_priority_fee_per_gas, is_dynamic_fee)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReadyCandidate {
    tx_id: TxId,
    effective_gas_price: u64,
    seq: u64,
}

impl ReadyCandidate {
    fn cmp_priority(&self, other: &Self) -> std::cmp::Ordering {
        self.effective_gas_price
            .cmp(&other.effective_gas_price)
            .then_with(|| other.seq.cmp(&self.seq))
            .then_with(|| other.tx_id.0.cmp(&self.tx_id.0))
    }
}

impl Ord for ReadyCandidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.cmp_priority(other)
    }
}

impl PartialOrd for ReadyCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn select_ready_candidates(
    state: &evm_db::stable_state::StableState,
    base_fee: u64,
    max_txs: usize,
) -> Vec<TxId> {
    if max_txs == 0 {
        return Vec::new();
    }
    let mut keys: Vec<ReadyKey> = Vec::new();
    for entry in state.ready_queue.range(..).take(READY_CANDIDATE_LIMIT) {
        keys.push(*entry.key());
    }

    let mut candidates: Vec<ReadyCandidate> = Vec::new();
    for key in keys {
        let tx_id = match state.ready_queue.get(&key) {
            Some(value) => value,
            None => continue,
        };
        let fields = load_fee_fields_and_seq(state, tx_id);
        let (max_fee_per_gas, max_priority_fee_per_gas, is_dynamic_fee, seq) = match fields {
            Ok(Some(value)) => value,
            Ok(None) | Err(RekeyError::DecodeFailed) => {
                candidates.push(ReadyCandidate {
                    tx_id,
                    effective_gas_price: 0,
                    seq: key.seq(),
                });
                continue;
            }
        };
        let effective_gas_price = compute_effective_gas_price(
            max_fee_per_gas,
            if is_dynamic_fee {
                max_priority_fee_per_gas
            } else {
                0
            },
            base_fee,
        )
        .unwrap_or(0);
        candidates.push(ReadyCandidate {
            tx_id,
            effective_gas_price,
            seq,
        });
    }

    select_top_k_ready_candidates(candidates, max_txs)
}

fn push_top_k_candidate(
    selected_heap: &mut BinaryHeap<Reverse<ReadyCandidate>>,
    candidate: ReadyCandidate,
    max_txs: usize,
) {
    if selected_heap.len() < max_txs {
        selected_heap.push(Reverse(candidate));
        return;
    }

    if let Some(worst_selected) = selected_heap.peek() {
        if candidate > worst_selected.0 {
            selected_heap.pop();
            selected_heap.push(Reverse(candidate));
        }
    }
}

fn select_top_k_ready_candidates(mut candidates: Vec<ReadyCandidate>, max_txs: usize) -> Vec<TxId> {
    if max_txs == 0 {
        return Vec::new();
    }

    let mut selected_heap: BinaryHeap<Reverse<ReadyCandidate>> = BinaryHeap::new();
    for candidate in candidates.drain(..) {
        push_top_k_candidate(&mut selected_heap, candidate, max_txs);
    }

    let mut selected: Vec<ReadyCandidate> =
        selected_heap.into_iter().map(|entry| entry.0).collect();
    selected.sort_by(|left, right| right.cmp_priority(left));
    selected
        .into_iter()
        .map(|candidate| candidate.tx_id)
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RekeyError {
    DecodeFailed,
}

#[cfg(test)]
mod tests {
    use super::should_stop_block_execution;

    #[test]
    fn should_stop_block_execution_stops_on_gas_limit() {
        assert!(should_stop_block_execution(10, 10, 0, 0, 0));
        assert!(should_stop_block_execution(11, 10, 0, 0, 0));
    }

    #[test]
    fn should_stop_block_execution_stops_on_instruction_budget() {
        assert!(should_stop_block_execution(1, 0, 5, 100, 105));
        assert!(should_stop_block_execution(1, 0, 5, 100, 120));
        assert!(!should_stop_block_execution(1, 0, 5, 100, 104));
    }

    #[test]
    fn should_stop_block_execution_ignores_instruction_budget_when_disabled() {
        assert!(!should_stop_block_execution(1, 0, 0, 100, 100_000));
    }
}
