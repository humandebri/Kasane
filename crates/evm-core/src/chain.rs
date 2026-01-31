//! どこで: Phase1のチェーン操作 / 何を: submit/produce/execute / なぜ: 同期Tx体験の基盤のため

use crate::base_fee::compute_next_base_fee;
use crate::hash;
use crate::revm_exec::{compute_effective_gas_price, execute_tx, ExecError};
use crate::state_root::compute_state_root;
use crate::tx_decode::decode_tx;
use evm_db::chain_data::constants::{
    CHAIN_ID, DEFAULT_BLOCK_GAS_LIMIT, DROP_CODE_CALLER_MISSING, DROP_CODE_DECODE, DROP_CODE_MISSING,
    DROP_CODE_INVALID_FEE, MAX_TX_SIZE,
};
use evm_db::chain_data::{
    BlockData, CallerKey, Head, ReceiptLike, ReadyKey, SenderKey, SenderNonceKey, TxEnvelope, TxId,
    TxIndexEntry, TxKind, TxLoc,
};
use evm_db::stable_state::{with_state, with_state_mut};
use evm_db::types::keys::make_account_key;
use evm_db::types::values::AccountVal;
use revm::primitives::Address;
use revm::primitives::U256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChainError {
    TxAlreadySeen,
    QueueEmpty,
    TxTooLarge,
    InvalidLimit,
    DecodeFailed,
    InvalidFee,
    NonceConflict,
    ExecFailed(Option<ExecError>),
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
}

pub struct PruneResult {
    pub did_work: bool,
    pub remaining: u64,
    pub pruned_before_block: Option<u64>,
}

pub fn submit_tx(kind: TxKind, tx_bytes: Vec<u8>) -> Result<TxId, ChainError> {
    let tx_id = TxId(hash::tx_id(&tx_bytes));
    with_state_mut(|state| {
        if tx_bytes.len() > MAX_TX_SIZE {
            return Err(ChainError::TxTooLarge);
        }
        if state.seen_tx.get(&tx_id).is_some() {
            return Err(ChainError::TxAlreadySeen);
        }
        let envelope = TxEnvelope::new(tx_id, kind, tx_bytes);
        let caller = match envelope.kind {
            TxKind::IcSynthetic => envelope.caller_evm.unwrap_or([0u8; 20]),
            TxKind::EthSigned => [0u8; 20],
        };
        let tx_env = decode_tx(envelope.kind, Address::from(caller), &envelope.tx_bytes)
            .map_err(|_| ChainError::DecodeFailed)?;
        let chain_state = state.chain_state.get();
        let base_fee = chain_state.base_fee;
        let min_gas_price = chain_state.min_gas_price;
        let min_priority_fee = chain_state.min_priority_fee;
        if !min_fee_satisfied(&tx_env, base_fee, min_priority_fee, min_gas_price) {
            return Err(ChainError::InvalidFee);
        }
        let effective_gas_price = compute_effective_gas_price(
            tx_env.gas_price,
            tx_env.gas_priority_fee.unwrap_or(0),
            base_fee,
        )
        .ok_or(ChainError::InvalidFee)?;
        state.seen_tx.insert(tx_id, 1);
        state.tx_store.insert(tx_id, envelope);
        let mut metrics = *state.metrics_state.get();
        metrics.record_submission(1);
        state.metrics_state.set(metrics);
        let mut meta = *state.queue_meta.get();
        let seq = meta.push();
        state.queue_meta.set(meta);
        state.tx_locs.insert(tx_id, TxLoc::queued(seq));
        let mut chain_state = *state.chain_state.get();
        chain_state.next_queue_seq = meta.tail;
        state.chain_state.set(chain_state);
        let sender_key = SenderKey::new(address_to_bytes(tx_env.caller));
        let pending_key = SenderNonceKey::new(sender_key.0, tx_env.nonce);
        if state.pending_by_sender_nonce.get(&pending_key).is_some() {
            return Err(ChainError::NonceConflict);
        }
        state.pending_by_sender_nonce.insert(pending_key, tx_id);
        state.pending_meta_by_tx_id.insert(tx_id, pending_key);
        promote_if_next_nonce(state, sender_key, tx_id, tx_env.nonce, effective_gas_price, seq)?;
        Ok(tx_id)
    })
}

pub fn submit_ic_tx(
    caller_evm: [u8; 20],
    caller_principal: Vec<u8>,
    canister_id: Vec<u8>,
    tx_bytes: Vec<u8>,
) -> Result<TxId, ChainError> {
    with_state_mut(|state| {
        if tx_bytes.len() > MAX_TX_SIZE {
            return Err(ChainError::TxTooLarge);
        }
        let caller_key = CallerKey::from_principal_bytes(&caller_principal);
        let current_nonce = state.caller_nonces.get(&caller_key).unwrap_or(0);
        let next_nonce = current_nonce.saturating_add(1);
        state.caller_nonces.insert(caller_key, next_nonce);
        let tx_id = TxId(hash::ic_synthetic_tx_id(
            CHAIN_ID,
            &canister_id,
            &caller_principal,
            current_nonce,
            &tx_bytes,
        ));
        if state.seen_tx.get(&tx_id).is_some() {
            return Err(ChainError::TxAlreadySeen);
        }
        let envelope = TxEnvelope::new_with_caller(tx_id, TxKind::IcSynthetic, tx_bytes, caller_evm);
        let tx_env = decode_tx(
            envelope.kind,
            Address::from(caller_evm),
            &envelope.tx_bytes,
        )
        .map_err(|_| ChainError::DecodeFailed)?;
        let chain_state = state.chain_state.get();
        let base_fee = chain_state.base_fee;
        let min_gas_price = chain_state.min_gas_price;
        let min_priority_fee = chain_state.min_priority_fee;
        if !min_fee_satisfied(&tx_env, base_fee, min_priority_fee, min_gas_price) {
            return Err(ChainError::InvalidFee);
        }
        let effective_gas_price = compute_effective_gas_price(
            tx_env.gas_price,
            tx_env.gas_priority_fee.unwrap_or(0),
            base_fee,
        )
        .ok_or(ChainError::InvalidFee)?;
        state.seen_tx.insert(tx_id, 1);
        state.tx_store.insert(tx_id, envelope);
        let mut metrics = *state.metrics_state.get();
        metrics.record_submission(1);
        state.metrics_state.set(metrics);
        let mut meta = *state.queue_meta.get();
        let seq = meta.push();
        state.queue_meta.set(meta);
        state.tx_locs.insert(tx_id, TxLoc::queued(seq));
        let mut chain_state = *state.chain_state.get();
        chain_state.next_queue_seq = meta.tail;
        state.chain_state.set(chain_state);
        let sender_key = SenderKey::new(address_to_bytes(tx_env.caller));
        let pending_key = SenderNonceKey::new(sender_key.0, tx_env.nonce);
        if state.pending_by_sender_nonce.get(&pending_key).is_some() {
            return Err(ChainError::NonceConflict);
        }
        state.pending_by_sender_nonce.insert(pending_key, tx_id);
        state.pending_meta_by_tx_id.insert(tx_id, pending_key);
        promote_if_next_nonce(state, sender_key, tx_id, tx_env.nonce, effective_gas_price, seq)?;
        Ok(tx_id)
    })
}

pub fn dev_mint(address: [u8; 20], amount: u128) -> Result<(), ChainError> {
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

pub fn produce_block(max_txs: usize) -> Result<BlockData, ChainError> {
    if max_txs == 0 {
        return Err(ChainError::InvalidLimit);
    }
    let head = with_state(|state| *state.head.get());
    let number = head.number.saturating_add(1);
    let timestamp = head.timestamp.saturating_add(1);
    let parent_hash = head.block_hash;

    let mut included: Vec<TxId> = Vec::new();
    let mut dropped_total = 0u64;
    let mut dropped_by_code = [0u64; evm_db::chain_data::metrics::DROP_CODE_SLOTS];
    with_state_mut(|state| {
        let base_fee = state.chain_state.get().base_fee;
        rekey_ready_queue_with_drop(state, base_fee, &mut dropped_total, &mut dropped_by_code);
    });
    let mut tx_ids = Vec::new();
    with_state_mut(|state| {
        if state.ready_queue.len() == 0 {
            return;
        }
        while tx_ids.len() < max_txs {
            let entry = match state.ready_queue.range(..).next() {
                Some(value) => value,
                None => break,
            };
            let key = *entry.key();
            let tx_id = entry.value();
            state.ready_queue.remove(&key);
            state.ready_key_by_tx_id.remove(&tx_id);
            tx_ids.push(tx_id);
        }
    });
    if tx_ids.is_empty() && dropped_total == 0 {
        return Err(ChainError::QueueEmpty);
    }
    let mut block_gas_used = 0u64;
    for tx_id in tx_ids {
        let envelope = with_state(|state| state.tx_store.get(&tx_id));
        let envelope = match envelope {
            Some(value) => value,
            None => {
                with_state_mut(|state| state.tx_locs.insert(tx_id, TxLoc::dropped(DROP_CODE_MISSING)));
                track_drop(&mut dropped_total, &mut dropped_by_code, DROP_CODE_MISSING);
                with_state_mut(|state| advance_sender_after_tx(state, tx_id, None, None));
                continue;
            }
        };
        let caller = match envelope.kind {
            TxKind::IcSynthetic => match envelope.caller_evm {
                Some(value) => value,
                None => {
                    with_state_mut(|state| {
                        state.tx_locs.insert(tx_id, TxLoc::dropped(DROP_CODE_CALLER_MISSING));
                    });
                    track_drop(&mut dropped_total, &mut dropped_by_code, DROP_CODE_CALLER_MISSING);
                    with_state_mut(|state| advance_sender_after_tx(state, tx_id, None, None));
                    continue;
                }
            },
            TxKind::EthSigned => [0u8; 20],
        };
        let tx_env = match decode_tx(envelope.kind, Address::from(caller), &envelope.tx_bytes) {
            Ok(value) => value,
            Err(_) => {
                with_state_mut(|state| state.tx_locs.insert(tx_id, TxLoc::dropped(DROP_CODE_DECODE)));
                track_drop(&mut dropped_total, &mut dropped_by_code, DROP_CODE_DECODE);
                with_state_mut(|state| advance_sender_after_tx(state, tx_id, Some(caller), None));
                continue;
            }
        };
        let sender_bytes = address_to_bytes(tx_env.caller);
        let sender_nonce = tx_env.nonce;
        let tx_index = u32::try_from(included.len()).unwrap_or(u32::MAX);
        let outcome = match execute_tx(tx_id, tx_index, tx_env, number, timestamp) {
            Ok(value) => value,
            Err(err) => {
                if err == ExecError::InvalidGasFee {
                    with_state_mut(|state| state.tx_locs.insert(tx_id, TxLoc::dropped(DROP_CODE_INVALID_FEE)));
                    track_drop(&mut dropped_total, &mut dropped_by_code, DROP_CODE_INVALID_FEE);
                    with_state_mut(|state| advance_sender_after_tx(state, tx_id, Some(sender_bytes), Some(sender_nonce)));
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
                    return_data_hash: hash::keccak256(&output),
                    return_data: output,
                    contract_address: None,
                    logs: Vec::new(),
                };
                with_state_mut(|state| {
                    state
                        .tx_index
                        .insert(tx_id, TxIndexEntry { block_number: number, tx_index });
                    state.receipts.insert(tx_id, receipt);
                    state.tx_locs.insert(tx_id, TxLoc::included(number, tx_index));
                });
                included.push(tx_id);
                with_state_mut(|state| {
                    advance_sender_after_tx(state, tx_id, Some(sender_bytes), Some(sender_nonce))
                });
                continue;
            }
        };
        with_state_mut(|state| {
            advance_sender_after_tx(state, tx_id, Some(sender_bytes), Some(sender_nonce))
        });
        block_gas_used = block_gas_used.saturating_add(outcome.receipt.gas_used);
        with_state_mut(|state| {
            state
                .tx_locs
                .insert(tx_id, TxLoc::included(number, outcome.tx_index));
        });
        included.push(tx_id);
    }

    with_state_mut(|state| {
        let mut metrics = *state.metrics_state.get();
        for (idx, count) in dropped_by_code.iter().enumerate() {
            if *count > 0 {
                metrics.record_drop(idx as u16, *count);
            }
        }
        if !included.is_empty() {
            metrics.record_included(included.len() as u64);
            metrics.record_block(number, timestamp, included.len() as u64, dropped_total);
        }
        state.metrics_state.set(metrics);
    });

    if included.is_empty() {
        return Err(ChainError::NoExecutableTx);
    }

    let mut tx_id_bytes = Vec::with_capacity(included.len());
    for tx_id in included.iter() {
        tx_id_bytes.push(tx_id.0);
    }
    let tx_list_hash = hash::tx_list_hash(&tx_id_bytes);
    let state_root = compute_state_root();
    let block_hash = hash::block_hash(parent_hash, number, timestamp, tx_list_hash, state_root);
    let block = BlockData::new(
        number,
        parent_hash,
        block_hash,
        timestamp,
        included,
        tx_list_hash,
        state_root,
    );

    with_state_mut(|state| {
        state.blocks.insert(number, block.clone());
        state.head.set(Head {
            number,
            block_hash,
            timestamp,
        });
        let mut chain_state = *state.chain_state.get();
        chain_state.last_block_number = number;
        chain_state.last_block_time = timestamp;
        chain_state.base_fee =
            compute_next_base_fee(chain_state.base_fee, block_gas_used, DEFAULT_BLOCK_GAS_LIMIT);
        state.chain_state.set(chain_state);
    });

    Ok(block)
}

pub fn execute_eth_raw_tx(raw_tx: Vec<u8>) -> Result<ExecResult, ChainError> {
    let tx_id = submit_tx(TxKind::EthSigned, raw_tx)?;
    let result = execute_and_seal(tx_id, TxKind::EthSigned)?;
    Ok(result)
}

pub fn execute_ic_tx(
    caller: [u8; 20],
    caller_principal: Vec<u8>,
    canister_id: Vec<u8>,
    tx_bytes: Vec<u8>,
) -> Result<ExecResult, ChainError> {
    let tx_id = submit_ic_tx(caller, caller_principal, canister_id, tx_bytes)?;
    execute_and_seal_with_caller(tx_id, TxKind::IcSynthetic, caller)
}

pub fn get_block(number: u64) -> Option<BlockData> {
    with_state(|state| state.blocks.get(&number))
}

pub fn get_head_number() -> u64 {
    with_state(|state| state.head.get().number)
}

pub fn get_receipt(tx_id: &TxId) -> Option<ReceiptLike> {
    with_state(|state| state.receipts.get(tx_id))
}

pub fn get_tx_envelope(tx_id: &TxId) -> Option<TxEnvelope> {
    with_state(|state| state.tx_store.get(tx_id))
}

fn execute_and_seal(tx_id: TxId, kind: TxKind) -> Result<ExecResult, ChainError> {
    execute_and_seal_with_caller(tx_id, kind, [0u8; 20])
}

fn execute_and_seal_with_caller(
    tx_id: TxId,
    kind: TxKind,
    caller: [u8; 20],
) -> Result<ExecResult, ChainError> {
    let envelope =
        with_state(|state| state.tx_store.get(&tx_id)).ok_or(ChainError::ExecFailed(None))?;

    let head = with_state(|state| *state.head.get());
    let number = head.number.saturating_add(1);
    let timestamp = head.timestamp.saturating_add(1);
    let parent_hash = head.block_hash;

    let tx_env = decode_tx(kind, Address::from(caller), &envelope.tx_bytes)
        .map_err(|_| ChainError::DecodeFailed)?;
    let sender_bytes = address_to_bytes(tx_env.caller);
    let sender_nonce = tx_env.nonce;

    let outcome = execute_tx(tx_id, 0, tx_env, number, timestamp)
        .map_err(|err| ChainError::ExecFailed(Some(err)))?;

    let tx_list_hash = hash::tx_list_hash(&[tx_id.0]);
    let block_hash = hash::block_hash(parent_hash, number, timestamp, tx_list_hash, compute_state_root());

    let block = BlockData::new(
        number,
        parent_hash,
        block_hash,
        timestamp,
        vec![tx_id],
        tx_list_hash,
        compute_state_root(),
    );

    with_state_mut(|state| {
        state.blocks.insert(number, block.clone());
        state.head.set(Head {
            number,
            block_hash,
            timestamp,
        });
        state
            .tx_locs
            .insert(tx_id, TxLoc::included(number, outcome.tx_index));
        advance_sender_after_tx(
            state,
            tx_id,
            Some(sender_bytes),
            Some(sender_nonce),
        );
        let mut chain_state = *state.chain_state.get();
        chain_state.last_block_number = number;
        chain_state.last_block_time = timestamp;
        chain_state.base_fee = compute_next_base_fee(
            chain_state.base_fee,
            outcome.receipt.gas_used,
            DEFAULT_BLOCK_GAS_LIMIT,
        );
        state.chain_state.set(chain_state);
        let mut metrics = *state.metrics_state.get();
        metrics.record_included(1);
        metrics.record_block(number, timestamp, 1, 0);
        state.metrics_state.set(metrics);
    });

    Ok(ExecResult {
        tx_id,
        block_number: number,
        tx_index: outcome.tx_index,
        status: outcome.receipt.status,
        gas_used: outcome.receipt.gas_used,
        return_data: outcome.return_data,
    })
}

pub fn get_tx_loc(tx_id: &TxId) -> Option<TxLoc> {
    with_state(|state| state.tx_locs.get(tx_id))
}

pub fn prune_blocks(retain: u64, max_ops: u32) -> Result<PruneResult, ChainError> {
    if retain == 0 || max_ops == 0 {
        return Err(ChainError::InvalidLimit);
    }
    with_state_mut(|state| {
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
            let block = match state.blocks.get(&next) {
                Some(value) => value,
                None => {
                    prune_state.set_pruned_before(next);
                    next = next.saturating_add(1);
                    did_work = true;
                    continue;
                }
            };
            let needed = 1u64 + (block.tx_ids.len() as u64).saturating_mul(4);
            if ops_used.saturating_add(needed) > max_ops {
                break;
            }
            let block = state.blocks.remove(&next).unwrap_or(block);
            for tx_id in block.tx_ids.iter() {
                state.receipts.remove(tx_id);
                state.tx_index.remove(tx_id);
                state.tx_locs.remove(tx_id);
                state.tx_store.remove(tx_id);
            }
            ops_used = ops_used.saturating_add(needed);
            prune_state.set_pruned_before(next);
            next = next.saturating_add(1);
            did_work = true;
        }
        prune_state.next_prune_block = next;
        state.prune_state.set(prune_state);
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
        let start = cursor.unwrap_or(0);
        let mut items = Vec::new();
        let mut next_cursor = None;
        let mut seen = 0u64;
        for entry in state.ready_queue.range(..) {
            if items.len() >= limit {
                next_cursor = Some(seen);
                break;
            }
            if seen < start {
                seen = seen.saturating_add(1);
                continue;
            }
            let seq = entry.key().seq();
            let tx_id = entry.value();
            let kind = state
                .tx_store
                .get(&tx_id)
                .map(|e| e.kind)
                .unwrap_or(TxKind::EthSigned);
            items.push(QueueItem { seq, tx_id, kind });
            seen = seen.saturating_add(1);
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

fn promote_if_next_nonce(
    state: &mut evm_db::stable_state::StableState,
    sender: SenderKey,
    tx_id: TxId,
    nonce: u64,
    effective_gas_price: u64,
    seq: u64,
) -> Result<(), ChainError> {
    match state.pending_min_nonce.get(&sender) {
        None => {
            state.pending_min_nonce.insert(sender, nonce);
            insert_ready(state, tx_id, effective_gas_price, seq)?;
        }
        Some(current) => {
            if nonce < current {
                let old_key = SenderNonceKey::new(sender.0, current);
                if let Some(old_tx_id) = state.pending_by_sender_nonce.get(&old_key) {
                    remove_ready_by_tx_id(state, old_tx_id);
                }
                state.pending_min_nonce.insert(sender, nonce);
                insert_ready(state, tx_id, effective_gas_price, seq)?;
            }
        }
    }
    Ok(())
}

fn insert_ready(
    state: &mut evm_db::stable_state::StableState,
    tx_id: TxId,
    effective_gas_price: u64,
    seq: u64,
) -> Result<(), ChainError> {
    let key = ReadyKey::new(u128::from(effective_gas_price), seq, tx_id.0);
    state.ready_queue.insert(key, tx_id);
    state.ready_key_by_tx_id.insert(tx_id, key);
    Ok(())
}

fn remove_ready_by_tx_id(state: &mut evm_db::stable_state::StableState, tx_id: TxId) {
    if let Some(key) = state.ready_key_by_tx_id.remove(&tx_id) {
        state.ready_queue.remove(&key);
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
    let sender = pending_key.sender;
    if state.pending_min_nonce.get(&sender) != Some(pending_key.nonce) {
        return;
    }
    let mut cursor_nonce = pending_key.nonce;
    loop {
        match next_pending_for_sender(state, sender, cursor_nonce) {
            Some((next_nonce, next_tx_id)) => {
                state.pending_min_nonce.insert(sender, next_nonce);
                if let Some((effective_gas_price, seq)) =
                    load_effective_fee_and_seq(state, next_tx_id)
                {
                    let _ = insert_ready(state, next_tx_id, effective_gas_price, seq);
                    return;
                }
                drop_invalid_fee_pending(state, next_tx_id, None, None);
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
    remove_ready_by_tx_id(state, tx_id);
    if let Some(pending_key) = state.pending_meta_by_tx_id.remove(&tx_id) {
        state.pending_by_sender_nonce.remove(&pending_key);
    }
    state.tx_locs.insert(tx_id, TxLoc::dropped(DROP_CODE_INVALID_FEE));
    if let (Some(total), Some(by_code)) = (dropped_total, dropped_by_code) {
        track_drop(total, by_code, DROP_CODE_INVALID_FEE);
    } else {
        let mut metrics = *state.metrics_state.get();
        metrics.record_drop(DROP_CODE_INVALID_FEE, 1);
        state.metrics_state.set(metrics);
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

fn load_effective_fee_and_seq(
    state: &mut evm_db::stable_state::StableState,
    tx_id: TxId,
) -> Option<(u64, u64)> {
    let envelope = match state.tx_store.get(&tx_id) {
        Some(value) => value,
        None => return None,
    };
    let caller = match envelope.kind {
        TxKind::IcSynthetic => match envelope.caller_evm {
            Some(value) => value,
            None => return None,
        },
        TxKind::EthSigned => [0u8; 20],
    };
    let tx_env = decode_tx(envelope.kind, Address::from(caller), &envelope.tx_bytes).ok()?;
    let chain_state = state.chain_state.get();
    let base_fee = chain_state.base_fee;
    let min_gas_price = chain_state.min_gas_price;
    let min_priority_fee = chain_state.min_priority_fee;
    if !min_fee_satisfied(&tx_env, base_fee, min_priority_fee, min_gas_price) {
        return None;
    }
    let effective_gas_price = compute_effective_gas_price(
        tx_env.gas_price,
        tx_env.gas_priority_fee.unwrap_or(0),
        base_fee,
    )?;
    let seq = match state.tx_locs.get(&tx_id) {
        Some(loc) => loc.seq,
        None => return None,
    };
    Some((effective_gas_price, seq))
}

fn address_to_bytes(address: Address) -> [u8; 20] {
    let mut out = [0u8; 20];
    out.copy_from_slice(address.as_ref());
    out
}

fn min_fee_satisfied(
    tx_env: &revm::context::TxEnv,
    base_fee: u64,
    min_priority_fee: u64,
    min_gas_price: u64,
) -> bool {
    if let Some(priority) = tx_env.gas_priority_fee {
        let min_priority_fee = u128::from(min_priority_fee);
        if priority < min_priority_fee {
            return false;
        }
        let base_fee = u128::from(base_fee);
        let base_plus_min = base_fee.saturating_add(min_priority_fee);
        let max_fee = tx_env.gas_price;
        max_fee >= base_fee && max_fee >= base_plus_min
    } else {
        tx_env.gas_price >= u128::from(min_gas_price)
    }
}

fn rekey_ready_queue_with_drop(
    state: &mut evm_db::stable_state::StableState,
    base_fee: u64,
    dropped_total: &mut u64,
    dropped_by_code: &mut [u64],
) {
    let mut updates: Vec<(ReadyKey, TxId, Option<ReadyKey>, Option<u16>)> = Vec::new();
    let mut keys: Vec<ReadyKey> = Vec::new();
    for entry in state.ready_queue.range(..) {
        keys.push(*entry.key());
    }
    for old_key in keys {
        let tx_id = match state.ready_queue.get(&old_key) {
            Some(value) => value,
            None => continue,
        };
        let effective = load_effective_fee_and_seq_with_base_fee(state, tx_id, base_fee);
        match effective {
            Ok(Some((fee, seq))) => {
                let new_key = ReadyKey::new(u128::from(fee), seq, tx_id.0);
                if new_key != old_key {
                    updates.push((old_key, tx_id, Some(new_key), None));
                }
            }
            Ok(None) => updates.push((old_key, tx_id, None, None)),
            Err(RekeyError::DecodeFailed) => {
                updates.push((old_key, tx_id, None, Some(DROP_CODE_DECODE)));
            }
        }
    }

    for (old_key, tx_id, next_key, drop_code) in updates {
        state.ready_queue.remove(&old_key);
        state.ready_key_by_tx_id.remove(&tx_id);
        match next_key {
            Some(new_key) => {
                state.ready_queue.insert(new_key, tx_id);
                state.ready_key_by_tx_id.insert(tx_id, new_key);
            }
            None => {
                let code = drop_code.unwrap_or(DROP_CODE_INVALID_FEE);
                state.tx_locs.insert(tx_id, TxLoc::dropped(code));
                track_drop(dropped_total, dropped_by_code, code);
                advance_sender_after_tx(state, tx_id, None, None);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RekeyError {
    DecodeFailed,
}

fn load_effective_fee_and_seq_with_base_fee(
    state: &mut evm_db::stable_state::StableState,
    tx_id: TxId,
    base_fee: u64,
) -> Result<Option<(u64, u64)>, RekeyError> {
    let envelope = match state.tx_store.get(&tx_id) {
        Some(value) => value,
        None => return Ok(None),
    };
    let caller = match envelope.kind {
        TxKind::IcSynthetic => match envelope.caller_evm {
            Some(value) => value,
            None => return Err(RekeyError::DecodeFailed),
        },
        TxKind::EthSigned => [0u8; 20],
    };
    let tx_env = decode_tx(envelope.kind, Address::from(caller), &envelope.tx_bytes)
        .map_err(|_| RekeyError::DecodeFailed)?;
    let min_gas_price = state.chain_state.get().min_gas_price;
    let min_priority_fee = state.chain_state.get().min_priority_fee;
    if !min_fee_satisfied(&tx_env, base_fee, min_priority_fee, min_gas_price) {
        return Ok(None);
    }
    let effective_gas_price = compute_effective_gas_price(
        tx_env.gas_price,
        tx_env.gas_priority_fee.unwrap_or(0),
        base_fee,
    )
    .ok_or(RekeyError::DecodeFailed)?;
    let seq = match state.tx_locs.get(&tx_id) {
        Some(loc) => loc.seq,
        None => return Ok(None),
    };
    Ok(Some((effective_gas_price, seq)))
}
