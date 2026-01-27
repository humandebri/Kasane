//! どこで: canister入口 / 何を: Phase1のAPI公開 / なぜ: ICPから同期Tx実行を提供するため

use candid::{CandidType, Principal};
use evm_db::meta::init_meta_or_trap;
use evm_db::phase1::{BlockData, ReceiptLike, TxId};
use evm_db::stable_state::init_stable_state;
use evm_db::upgrade;
use evm_core::chain;
use evm_core::hash::keccak256;
use serde::Deserialize;

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ExecResultDto {
    pub tx_id: Vec<u8>,
    pub block_number: u64,
    pub tx_index: u32,
    pub status: u8,
    pub gas_used: u64,
    pub return_data: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct BlockView {
    pub number: u64,
    pub parent_hash: Vec<u8>,
    pub block_hash: Vec<u8>,
    pub timestamp: u64,
    pub tx_ids: Vec<Vec<u8>>,
    pub tx_list_hash: Vec<u8>,
    pub state_root: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ReceiptView {
    pub tx_id: Vec<u8>,
    pub block_number: u64,
    pub tx_index: u32,
    pub status: u8,
    pub gas_used: u64,
    pub return_data_hash: Vec<u8>,
    pub contract_address: Option<Vec<u8>>,
}

#[ic_cdk::init]
fn init() {
    init_meta_or_trap();
    init_stable_state();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    upgrade::post_upgrade();
    init_meta_or_trap();
    init_stable_state();
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    upgrade::pre_upgrade();
}

#[ic_cdk::update]
fn execute_eth_raw_tx(raw_tx: Vec<u8>) -> ExecResultDto {
    let result = chain::execute_eth_raw_tx(raw_tx)
        .unwrap_or_else(|_| ic_cdk::trap("execute_eth_raw_tx failed"));
    ExecResultDto {
        tx_id: result.tx_id.0.to_vec(),
        block_number: result.block_number,
        tx_index: result.tx_index,
        status: result.status,
        gas_used: result.gas_used,
        return_data: result.return_data,
    }
}

#[ic_cdk::update]
fn execute_ic_tx(tx_bytes: Vec<u8>) -> ExecResultDto {
    let caller = principal_to_evm_address(ic_cdk::api::msg_caller());
    let result = chain::execute_ic_tx(caller, tx_bytes)
        .unwrap_or_else(|_| ic_cdk::trap("execute_ic_tx failed"));
    ExecResultDto {
        tx_id: result.tx_id.0.to_vec(),
        block_number: result.block_number,
        tx_index: result.tx_index,
        status: result.status,
        gas_used: result.gas_used,
        return_data: result.return_data,
    }
}

#[ic_cdk::update]
fn submit_eth_tx(raw_tx: Vec<u8>) -> Vec<u8> {
    let tx_id = chain::submit_tx(evm_db::phase1::TxKind::EthSigned, raw_tx)
        .unwrap_or_else(|_| ic_cdk::trap("submit_eth_tx failed"));
    tx_id.0.to_vec()
}

#[ic_cdk::update]
fn submit_ic_tx(tx_bytes: Vec<u8>) -> Vec<u8> {
    let tx_id = chain::submit_tx(evm_db::phase1::TxKind::IcSynthetic, tx_bytes)
        .unwrap_or_else(|_| ic_cdk::trap("submit_ic_tx failed"));
    tx_id.0.to_vec()
}

#[ic_cdk::update]
fn produce_block(max_txs: u32) -> BlockView {
    let limit = usize::try_from(max_txs).unwrap_or(0);
    let block = chain::produce_block(limit)
        .unwrap_or_else(|_| ic_cdk::trap("produce_block failed"));
    block_to_view(block)
}

#[ic_cdk::query]
fn get_block(number: u64) -> Option<BlockView> {
    chain::get_block(number).map(block_to_view)
}

#[ic_cdk::query]
fn get_receipt(tx_id: Vec<u8>) -> Option<ReceiptView> {
    if tx_id.len() != 32 {
        return None;
    }
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&tx_id);
    let receipt = chain::get_receipt(&TxId(buf))?;
    Some(receipt_to_view(receipt))
}

fn principal_to_evm_address(principal: Principal) -> [u8; 20] {
    let hash = keccak256(principal.as_slice());
    let mut out = [0u8; 20];
    out.copy_from_slice(&hash[12..32]);
    out
}

fn block_to_view(block: BlockData) -> BlockView {
    let mut tx_ids = Vec::with_capacity(block.tx_ids.len());
    for tx_id in block.tx_ids.into_iter() {
        tx_ids.push(tx_id.0.to_vec());
    }
    BlockView {
        number: block.number,
        parent_hash: block.parent_hash.to_vec(),
        block_hash: block.block_hash.to_vec(),
        timestamp: block.timestamp,
        tx_ids,
        tx_list_hash: block.tx_list_hash.to_vec(),
        state_root: block.state_root.to_vec(),
    }
}

fn receipt_to_view(receipt: ReceiptLike) -> ReceiptView {
    ReceiptView {
        tx_id: receipt.tx_id.0.to_vec(),
        block_number: receipt.block_number,
        tx_index: receipt.tx_index,
        status: receipt.status,
        gas_used: receipt.gas_used,
        return_data_hash: receipt.return_data_hash.to_vec(),
        contract_address: receipt.contract_address.map(|v| v.to_vec()),
    }
}
