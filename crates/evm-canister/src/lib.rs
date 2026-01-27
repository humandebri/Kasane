//! どこで: canister入口 / 何を: Phase1のAPI公開 / なぜ: ICPから同期Tx実行を提供するため

use candid::{CandidType, Principal};
use evm_db::meta::init_meta_or_trap;
use evm_db::chain_data::constants::MAX_RETURN_DATA;
use evm_db::chain_data::{BlockData, ReceiptLike, TxId, TxKind, TxLoc, TxLocKind};
use evm_db::stable_state::init_stable_state;
use evm_db::upgrade;
use evm_core::chain;
use evm_core::hash::keccak256;
use serde::Deserialize;

#[cfg(target_arch = "wasm32")]
getrandom::register_custom_getrandom!(always_fail_getrandom);

#[cfg(target_arch = "wasm32")]
fn always_fail_getrandom(_buf: &mut [u8]) -> Result<(), getrandom::Error> {
    Err(getrandom::Error::UNSUPPORTED)
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ExecResultDto {
    pub tx_id: Vec<u8>,
    pub block_number: u64,
    pub tx_index: u32,
    pub status: u8,
    pub gas_used: u64,
    pub return_data: Option<Vec<u8>>,
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
    pub effective_gas_price: u64,
    pub return_data_hash: Vec<u8>,
    pub return_data: Option<Vec<u8>>,
    pub contract_address: Option<Vec<u8>>,
    pub logs: Vec<LogView>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct LogView {
    pub address: Vec<u8>,
    pub topics: Vec<Vec<u8>>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct QueueItemView {
    pub seq: u64,
    pub tx_id: Vec<u8>,
    pub kind: TxKindView,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct QueueSnapshotView {
    pub items: Vec<QueueItemView>,
    pub next_cursor: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum PendingStatusView {
    Queued { seq: u64 },
    Included { block_number: u64, tx_index: u32 },
    Dropped { code: u16 },
    Unknown,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum TxKindView {
    EthSigned,
    IcSynthetic,
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
        return_data: clamp_return_data(result.return_data),
    }
}

#[ic_cdk::update]
fn execute_ic_tx(tx_bytes: Vec<u8>) -> ExecResultDto {
    let caller = principal_to_evm_address(ic_cdk::api::msg_caller());
    let caller_principal = ic_cdk::api::msg_caller().as_slice().to_vec();
    let canister_id = ic_cdk::api::canister_self().as_slice().to_vec();
    let result = chain::execute_ic_tx(caller, caller_principal, canister_id, tx_bytes)
        .unwrap_or_else(|_| ic_cdk::trap("execute_ic_tx failed"));
    ExecResultDto {
        tx_id: result.tx_id.0.to_vec(),
        block_number: result.block_number,
        tx_index: result.tx_index,
        status: result.status,
        gas_used: result.gas_used,
        return_data: clamp_return_data(result.return_data),
    }
}

#[ic_cdk::update]
fn submit_eth_tx(raw_tx: Vec<u8>) -> Vec<u8> {
    let tx_id = chain::submit_tx(evm_db::chain_data::TxKind::EthSigned, raw_tx)
        .unwrap_or_else(|_| ic_cdk::trap("submit_eth_tx failed"));
    schedule_mining();
    tx_id.0.to_vec()
}

#[ic_cdk::update]
fn submit_ic_tx(tx_bytes: Vec<u8>) -> Vec<u8> {
    let caller_principal = ic_cdk::api::msg_caller().as_slice().to_vec();
    let canister_id = ic_cdk::api::canister_self().as_slice().to_vec();
    let tx_id = chain::submit_ic_tx(caller_principal, canister_id, tx_bytes)
        .unwrap_or_else(|_| ic_cdk::trap("submit_ic_tx failed"));
    schedule_mining();
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

#[ic_cdk::query]
fn get_cycle_balance() -> u128 {
    ic_cdk::api::canister_cycle_balance()
}

#[ic_cdk::update]
fn set_auto_mine(enabled: bool) {
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.auto_mine_enabled = enabled;
        state.chain_state.set(chain_state);
    });
    if enabled {
        schedule_mining();
    }
}

#[ic_cdk::update]
fn set_mining_interval_ms(interval_ms: u64) {
    if interval_ms == 0 {
        ic_cdk::trap("mining interval must be > 0");
    }
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.mining_interval_ms = interval_ms;
        state.chain_state.set(chain_state);
    });
    schedule_mining();
}

#[ic_cdk::query]
fn get_pending(tx_id: Vec<u8>) -> PendingStatusView {
    if tx_id.len() != 32 {
        return PendingStatusView::Unknown;
    }
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&tx_id);
    let loc = chain::get_tx_loc(&TxId(buf));
    pending_to_view(loc)
}

#[ic_cdk::query]
fn get_queue_snapshot(limit: u32, cursor: Option<u64>) -> QueueSnapshotView {
    let limit = usize::try_from(limit).unwrap_or(0);
    let snapshot = chain::get_queue_snapshot(limit, cursor);
    let mut items = Vec::with_capacity(snapshot.items.len());
    for item in snapshot.items.into_iter() {
        items.push(QueueItemView {
            seq: item.seq,
            tx_id: item.tx_id.0.to_vec(),
            kind: tx_kind_to_view(item.kind),
        });
    }
    QueueSnapshotView {
        items,
        next_cursor: snapshot.next_cursor,
    }
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

fn clamp_return_data(return_data: Vec<u8>) -> Option<Vec<u8>> {
    if return_data.len() > MAX_RETURN_DATA {
        return None;
    }
    Some(return_data)
}

ic_cdk::export_candid!();

#[cfg(test)]
mod tests {
    use super::clamp_return_data;
    use evm_db::chain_data::constants::MAX_RETURN_DATA;

    #[test]
    fn clamp_return_data_rejects_oversize() {
        let data = vec![0u8; MAX_RETURN_DATA + 1];
        assert_eq!(clamp_return_data(data), None);
    }

    #[test]
    fn clamp_return_data_allows_limit() {
        let data = vec![7u8; MAX_RETURN_DATA];
        let out = clamp_return_data(data.clone());
        assert_eq!(out, Some(data));
    }
}

fn receipt_to_view(receipt: ReceiptLike) -> ReceiptView {
    ReceiptView {
        tx_id: receipt.tx_id.0.to_vec(),
        block_number: receipt.block_number,
        tx_index: receipt.tx_index,
        status: receipt.status,
        gas_used: receipt.gas_used,
        effective_gas_price: receipt.effective_gas_price,
        return_data_hash: receipt.return_data_hash.to_vec(),
        return_data: clamp_return_data(receipt.return_data),
        contract_address: receipt.contract_address.map(|v| v.to_vec()),
        logs: receipt.logs.into_iter().map(log_to_view).collect(),
    }
}

fn log_to_view(log: evm_db::chain_data::receipt::LogEntry) -> LogView {
    LogView {
        address: log.address.to_vec(),
        topics: log.topics.into_iter().map(|t| t.to_vec()).collect(),
        data: log.data,
    }
}

fn tx_kind_to_view(kind: TxKind) -> TxKindView {
    match kind {
        TxKind::EthSigned => TxKindView::EthSigned,
        TxKind::IcSynthetic => TxKindView::IcSynthetic,
    }
}

fn pending_to_view(loc: Option<TxLoc>) -> PendingStatusView {
    match loc {
        Some(TxLoc {
            kind: TxLocKind::Queued,
            seq,
            ..
        }) => PendingStatusView::Queued { seq },
        Some(TxLoc {
            kind: TxLocKind::Included,
            block_number,
            tx_index,
            ..
        }) => PendingStatusView::Included {
            block_number,
            tx_index,
        },
        Some(TxLoc {
            kind: TxLocKind::Dropped,
            drop_code,
            ..
        }) => PendingStatusView::Dropped { code: drop_code },
        None => PendingStatusView::Unknown,
    }
}

fn schedule_mining() {
    let interval_ms = evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        if !chain_state.auto_mine_enabled {
            return None;
        }
        if chain_state.mining_scheduled {
            return None;
        }
        chain_state.mining_scheduled = true;
        let interval_ms = chain_state.mining_interval_ms;
        state.chain_state.set(chain_state);
        Some(interval_ms)
    });
    if let Some(interval_ms) = interval_ms {
        ic_cdk_timers::set_timer(std::time::Duration::from_millis(interval_ms), || {
            mining_tick();
        });
    }
}

fn mining_tick() {
    let should_produce = evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.mining_scheduled = false;
        if !chain_state.auto_mine_enabled {
            state.chain_state.set(chain_state);
            return false;
        }
        if chain_state.is_producing {
            state.chain_state.set(chain_state);
            return false;
        }
        if state.queue_meta.get().is_empty() {
            state.chain_state.set(chain_state);
            return false;
        }
        chain_state.is_producing = true;
        state.chain_state.set(chain_state);
        true
    });

    if should_produce {
        let _ = chain::produce_block(evm_db::chain_data::MAX_TXS_PER_BLOCK);

        evm_db::stable_state::with_state_mut(|state| {
            let mut chain_state = *state.chain_state.get();
            chain_state.is_producing = false;
            state.chain_state.set(chain_state);
        });
    }
    schedule_mining();
}
