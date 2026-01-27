//! どこで: Phase1のチェーン操作 / 何を: submit/produce/execute / なぜ: 同期Tx体験の基盤のため

use crate::hash;
use crate::revm_exec::execute_tx;
use crate::state_root::{compute_state_root, compute_state_root_with};
use crate::tx_decode::decode_tx;
use evm_db::chain_data::{BlockData, Head, ReceiptLike, TxEnvelope, TxId, TxIndexEntry, TxKind};
use evm_db::stable_state::{with_state, with_state_mut};
use revm::primitives::Address;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChainError {
    TxAlreadySeen,
    QueueEmpty,
    TxTooLarge,
    InvalidLimit,
    DecodeFailed,
    ExecFailed,
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

pub fn submit_tx(kind: TxKind, tx_bytes: Vec<u8>) -> Result<TxId, ChainError> {
    let tx_id = TxId(hash::tx_id(&tx_bytes));
    with_state_mut(|state| {
        if state.seen_tx.get(&tx_id).is_some() {
            return Err(ChainError::TxAlreadySeen);
        }
        let envelope = TxEnvelope::new(tx_id, kind, tx_bytes);
        state.seen_tx.insert(tx_id, 1);
        state.tx_store.insert(tx_id, envelope);
        let mut meta = *state.queue_meta.get();
        let index = meta.push();
        state.queue_meta.set(meta);
        state.queue.insert(index, tx_id);
        Ok(tx_id)
    })
}

pub fn produce_block(max_txs: usize) -> Result<BlockData, ChainError> {
    with_state_mut(|state| {
        if max_txs == 0 {
            return Err(ChainError::InvalidLimit);
        }
        let mut meta = *state.queue_meta.get();
        if meta.is_empty() {
            return Err(ChainError::QueueEmpty);
        }

        let mut tx_ids: Vec<TxId> = Vec::new();
        while tx_ids.len() < max_txs {
            let index = match meta.pop() {
                Some(value) => value,
                None => break,
            };
            let tx_id = match state.queue.remove(&index) {
                Some(value) => value,
                None => continue,
            };
            tx_ids.push(tx_id);
        }
        state.queue_meta.set(meta);

        let head = *state.head.get();
        let number = head.number.saturating_add(1);
        let timestamp = head.timestamp.saturating_add(1);
        let parent_hash = head.block_hash;

        let mut tx_id_bytes = Vec::with_capacity(tx_ids.len());
        for tx_id in tx_ids.iter() {
            tx_id_bytes.push(tx_id.0);
        }
        let tx_list_hash = hash::tx_list_hash(&tx_id_bytes);
        let state_root = compute_state_root_with(state);
        let empty_return_hash = hash::keccak256(&[]);
        let block_hash = hash::block_hash(parent_hash, number, timestamp, tx_list_hash, state_root);

        for (index, tx_id) in tx_ids.iter().enumerate() {
            let tx_index = u32::try_from(index).unwrap_or(0);
            state.tx_index.insert(
                *tx_id,
                TxIndexEntry {
                    block_number: number,
                    tx_index,
                },
            );
            let receipt = ReceiptLike {
                tx_id: *tx_id,
                block_number: number,
                tx_index,
                status: 1,
                gas_used: 0,
                return_data_hash: empty_return_hash,
                contract_address: None,
            };
            state.receipts.insert(*tx_id, receipt);
        }

        let block = BlockData::new(
            number,
            parent_hash,
            block_hash,
            timestamp,
            tx_ids,
            tx_list_hash,
            state_root,
        );
        state.blocks.insert(number, block.clone());
        state.head.set(Head {
            number,
            block_hash,
            timestamp,
        });
        Ok(block)
    })
}

pub fn execute_eth_raw_tx(raw_tx: Vec<u8>) -> Result<ExecResult, ChainError> {
    let tx_id = submit_tx(TxKind::EthSigned, raw_tx)?;
    let result = execute_and_seal(tx_id, TxKind::EthSigned)?;
    Ok(result)
}

pub fn execute_ic_tx(caller: [u8; 20], tx_bytes: Vec<u8>) -> Result<ExecResult, ChainError> {
    let tx_id = submit_tx(TxKind::IcSynthetic, tx_bytes)?;
    execute_and_seal_with_caller(tx_id, TxKind::IcSynthetic, caller)
}

pub fn get_block(number: u64) -> Option<BlockData> {
    with_state(|state| state.blocks.get(&number))
}

pub fn get_receipt(tx_id: &TxId) -> Option<ReceiptLike> {
    with_state(|state| state.receipts.get(tx_id))
}

fn execute_and_seal(tx_id: TxId, kind: TxKind) -> Result<ExecResult, ChainError> {
    execute_and_seal_with_caller(tx_id, kind, [0u8; 20])
}

fn execute_and_seal_with_caller(
    tx_id: TxId,
    kind: TxKind,
    caller: [u8; 20],
) -> Result<ExecResult, ChainError> {
    let envelope = with_state(|state| state.tx_store.get(&tx_id)).ok_or(ChainError::ExecFailed)?;

    let head = with_state(|state| *state.head.get());
    let number = head.number.saturating_add(1);
    let timestamp = head.timestamp.saturating_add(1);
    let parent_hash = head.block_hash;

    let tx_env = decode_tx(kind, Address::from(caller), &envelope.tx_bytes)
        .map_err(|_| ChainError::DecodeFailed)?;

    let outcome = execute_tx(tx_id, 0, tx_env, number, timestamp).map_err(|_| ChainError::ExecFailed)?;

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
