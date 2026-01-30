//! どこで: Phase1のチェーン操作 / 何を: submit/produce/execute / なぜ: 同期Tx体験の基盤のため

use crate::hash;
use crate::revm_exec::{execute_tx, ExecError};
use crate::state_root::compute_state_root;
use crate::tx_decode::decode_tx;
use evm_db::chain_data::constants::{
    CHAIN_ID, DROP_CODE_CALLER_MISSING, DROP_CODE_DECODE, DROP_CODE_MISSING,
};
use evm_db::chain_data::{
    BlockData, CallerKey, Head, ReceiptLike, TxEnvelope, TxId, TxIndexEntry, TxKind, TxLoc,
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
        if state.seen_tx.get(&tx_id).is_some() {
            return Err(ChainError::TxAlreadySeen);
        }
        let envelope = TxEnvelope::new(tx_id, kind, tx_bytes);
        state.seen_tx.insert(tx_id, 1);
        state.tx_store.insert(tx_id, envelope);
        let mut metrics = *state.metrics_state.get();
        metrics.record_submission(1);
        state.metrics_state.set(metrics);
        let mut meta = *state.queue_meta.get();
        let index = meta.push();
        state.queue_meta.set(meta);
        state.tx_locs.insert(tx_id, TxLoc::queued(index));
        let mut chain_state = *state.chain_state.get();
        chain_state.next_queue_seq = meta.tail;
        state.chain_state.set(chain_state);
        state.queue.insert(index, tx_id);
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
        state.seen_tx.insert(tx_id, 1);
        state.tx_store.insert(tx_id, envelope);
        let mut metrics = *state.metrics_state.get();
        metrics.record_submission(1);
        state.metrics_state.set(metrics);
        let mut meta = *state.queue_meta.get();
        let index = meta.push();
        state.queue_meta.set(meta);
        state.tx_locs.insert(tx_id, TxLoc::queued(index));
        let mut chain_state = *state.chain_state.get();
        chain_state.next_queue_seq = meta.tail;
        state.chain_state.set(chain_state);
        state.queue.insert(index, tx_id);
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
    let tx_ids = with_state_mut(|state| {
        let mut meta = *state.queue_meta.get();
        if meta.is_empty() {
            return Err(ChainError::QueueEmpty);
        }
        let mut ids: Vec<TxId> = Vec::new();
        while ids.len() < max_txs {
            let index = match meta.pop() {
                Some(value) => value,
                None => break,
            };
            let tx_id = match state.queue.remove(&index) {
                Some(value) => value,
                None => continue,
            };
            ids.push(tx_id);
        }
        state.queue_meta.set(meta);
        Ok(ids)
    })?;
    if tx_ids.is_empty() {
        return Err(ChainError::QueueEmpty);
    }

    let head = with_state(|state| *state.head.get());
    let number = head.number.saturating_add(1);
    let timestamp = head.timestamp.saturating_add(1);
    let parent_hash = head.block_hash;

    let mut included: Vec<TxId> = Vec::new();
    let mut dropped_total = 0u64;
    let mut dropped_by_code = [0u64; evm_db::chain_data::metrics::DROP_CODE_SLOTS];
    for tx_id in tx_ids {
        let envelope = with_state(|state| state.tx_store.get(&tx_id));
        let envelope = match envelope {
            Some(value) => value,
            None => {
                with_state_mut(|state| state.tx_locs.insert(tx_id, TxLoc::dropped(DROP_CODE_MISSING)));
                track_drop(&mut dropped_total, &mut dropped_by_code, DROP_CODE_MISSING);
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
                continue;
            }
        };
        let tx_index = u32::try_from(included.len()).unwrap_or(u32::MAX);
        let outcome = match execute_tx(tx_id, tx_index, tx_env, number, timestamp) {
            Ok(value) => value,
            Err(_err) => {
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
                continue;
            }
        };
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
        let start = cursor.unwrap_or_else(|| state.queue_meta.get().head);
        let mut items = Vec::new();
        let mut next_cursor = None;
        for entry in state.queue.range(start..) {
            if items.len() >= limit {
                next_cursor = Some(*entry.key());
                break;
            }
            let seq = *entry.key();
            let tx_id = entry.value();
            let kind = state
                .tx_store
                .get(&tx_id)
                .map(|e| e.kind)
                .unwrap_or(TxKind::EthSigned);
            items.push(QueueItem { seq, tx_id, kind });
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
