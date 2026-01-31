//! どこで: canister入口 / 何を: Phase1のAPI公開 / なぜ: ICPから同期Tx実行を提供するため

use candid::{CandidType, Principal};
use ic_cdk::api::debug_print;
use evm_db::meta::init_meta_or_trap;
use evm_db::chain_data::constants::MAX_RETURN_DATA;
use evm_db::chain_data::constants::CHAIN_ID;
use evm_db::chain_data::{BlockData, ReceiptLike, TxEnvelope, TxId, TxKind, TxLoc, TxLocKind};
use evm_db::stable_state::{init_stable_state, with_state};
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
pub enum LookupError {
    NotFound,
    Pending,
    Pruned { pruned_before_block: u64 },
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ProduceBlockStatus {
    Produced {
        block_number: u64,
        txs: u32,
        gas_used: u64,
        dropped: u32,
    },
    NoOp {
        reason: NoOpReason,
    },
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum NoOpReason {
    NoExecutableTx,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ProduceBlockError {
    Internal(String),
    InvalidArgument(String),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum SubmitTxError {
    InvalidArgument(String),
    Rejected(String),
    Internal(String),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ExecuteTxError {
    InvalidArgument(String),
    Rejected(String),
    Internal(String),
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
pub struct HealthView {
    pub tip_number: u64,
    pub tip_hash: Vec<u8>,
    pub last_block_time: u64,
    pub queue_len: u64,
    pub auto_mine_enabled: bool,
    pub is_producing: bool,
    pub mining_scheduled: bool,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct DropCountView {
    pub code: u16,
    pub count: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct MetricsView {
    pub window: u64,
    pub blocks: u64,
    pub txs: u64,
    pub avg_txs_per_block: u64,
    pub block_rate_per_sec_x1000: Option<u64>,
    pub ema_block_rate_per_sec_x1000: u64,
    pub ema_txs_per_block_x1000: u64,
    pub queue_len: u64,
    pub drop_counts: Vec<DropCountView>,
    pub total_submitted: u64,
    pub total_included: u64,
    pub total_dropped: u64,
    pub cycles: u128,
    pub pruned_before_block: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct PruneResultView {
    pub did_work: bool,
    pub remaining: u64,
    pub pruned_before_block: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum EthTxListView {
    Hashes(Vec<Vec<u8>>),
    Full(Vec<EthTxView>),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct EthBlockView {
    pub number: u64,
    pub parent_hash: Vec<u8>,
    pub block_hash: Vec<u8>,
    pub timestamp: u64,
    pub txs: EthTxListView,
    pub state_root: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct EthTxView {
    pub hash: Vec<u8>,
    pub kind: TxKindView,
    pub raw: Vec<u8>,
    pub decoded: Option<DecodedTxView>,
    pub decode_ok: bool,
    pub block_number: Option<u64>,
    pub tx_index: Option<u32>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct DecodedTxView {
    pub from: Vec<u8>,
    pub to: Option<Vec<u8>>,
    pub nonce: u64,
    pub value: Vec<u8>,
    pub input: Vec<u8>,
    pub gas_limit: u64,
    pub gas_price: u128,
    pub chain_id: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct EthReceiptView {
    pub tx_hash: Vec<u8>,
    pub block_number: u64,
    pub tx_index: u32,
    pub status: u8,
    pub gas_used: u64,
    pub effective_gas_price: u64,
    pub contract_address: Option<Vec<u8>>,
    pub logs: Vec<LogView>,
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
fn execute_eth_raw_tx(raw_tx: Vec<u8>) -> Result<ExecResultDto, ExecuteTxError> {
    let result = match chain::execute_eth_raw_tx(raw_tx) {
        Ok(value) => value,
        Err(chain::ChainError::DecodeFailed) => {
            return Err(ExecuteTxError::InvalidArgument("decode failed".to_string()));
        }
        Err(chain::ChainError::TxTooLarge) => {
            return Err(ExecuteTxError::InvalidArgument("tx too large".to_string()));
        }
        Err(chain::ChainError::TxAlreadySeen) => {
            return Err(ExecuteTxError::Rejected("tx already seen".to_string()));
        }
        Err(chain::ChainError::InvalidFee) => {
            return Err(ExecuteTxError::Rejected("invalid fee".to_string()));
        }
        Err(chain::ChainError::NonceConflict) => {
            return Err(ExecuteTxError::Rejected("nonce conflict".to_string()));
        }
        Err(chain::ChainError::ExecFailed(_)) => {
            return Err(ExecuteTxError::Rejected("execution failed".to_string()));
        }
        Err(err) => {
            debug_print(format!("execute_eth_raw_tx err: {:?}", err));
            return Err(ExecuteTxError::Internal(format!("{:?}", err)));
        }
    };
    Ok(ExecResultDto {
        tx_id: result.tx_id.0.to_vec(),
        block_number: result.block_number,
        tx_index: result.tx_index,
        status: result.status,
        gas_used: result.gas_used,
        return_data: clamp_return_data(result.return_data),
    })
}

#[ic_cdk::update]
fn execute_ic_tx(tx_bytes: Vec<u8>) -> Result<ExecResultDto, ExecuteTxError> {
    let caller = principal_to_evm_address(ic_cdk::api::msg_caller());
    let caller_principal = ic_cdk::api::msg_caller().as_slice().to_vec();
    let canister_id = ic_cdk::api::canister_self().as_slice().to_vec();
    let result = match chain::execute_ic_tx(caller, caller_principal, canister_id, tx_bytes) {
        Ok(value) => value,
        Err(chain::ChainError::DecodeFailed) => {
            return Err(ExecuteTxError::InvalidArgument("decode failed".to_string()));
        }
        Err(chain::ChainError::TxTooLarge) => {
            return Err(ExecuteTxError::InvalidArgument("tx too large".to_string()));
        }
        Err(chain::ChainError::TxAlreadySeen) => {
            return Err(ExecuteTxError::Rejected("tx already seen".to_string()));
        }
        Err(chain::ChainError::InvalidFee) => {
            return Err(ExecuteTxError::Rejected("invalid fee".to_string()));
        }
        Err(chain::ChainError::NonceConflict) => {
            return Err(ExecuteTxError::Rejected("nonce conflict".to_string()));
        }
        Err(chain::ChainError::ExecFailed(_)) => {
            return Err(ExecuteTxError::Rejected("execution failed".to_string()));
        }
        Err(err) => {
            debug_print(format!("execute_ic_tx err: {:?}", err));
            return Err(ExecuteTxError::Internal(format!("{:?}", err)));
        }
    };
    Ok(ExecResultDto {
        tx_id: result.tx_id.0.to_vec(),
        block_number: result.block_number,
        tx_index: result.tx_index,
        status: result.status,
        gas_used: result.gas_used,
        return_data: clamp_return_data(result.return_data),
    })
}

#[ic_cdk::update]
fn submit_eth_tx(raw_tx: Vec<u8>) -> Result<Vec<u8>, SubmitTxError> {
    let tx_id = match chain::submit_tx(evm_db::chain_data::TxKind::EthSigned, raw_tx) {
        Ok(value) => value,
        Err(chain::ChainError::TxTooLarge) => {
            return Err(SubmitTxError::InvalidArgument("tx too large".to_string()));
        }
        Err(chain::ChainError::DecodeFailed) => {
            return Err(SubmitTxError::InvalidArgument("decode failed".to_string()));
        }
        Err(chain::ChainError::TxAlreadySeen) => {
            return Err(SubmitTxError::Rejected("tx already seen".to_string()));
        }
        Err(chain::ChainError::InvalidFee) => {
            return Err(SubmitTxError::Rejected("invalid fee".to_string()));
        }
        Err(chain::ChainError::NonceConflict) => {
            return Err(SubmitTxError::Rejected("nonce conflict".to_string()));
        }
        Err(err) => {
            debug_print(format!("submit_eth_tx err: {:?}", err));
            return Err(SubmitTxError::Internal(format!("{:?}", err)));
        }
    };
    schedule_mining();
    Ok(tx_id.0.to_vec())
}

#[ic_cdk::update]
fn submit_ic_tx(tx_bytes: Vec<u8>) -> Result<Vec<u8>, SubmitTxError> {
    let caller = principal_to_evm_address(ic_cdk::api::msg_caller());
    let caller_principal = ic_cdk::api::msg_caller().as_slice().to_vec();
    let canister_id = ic_cdk::api::canister_self().as_slice().to_vec();
    let tx_id = match chain::submit_ic_tx(caller, caller_principal, canister_id, tx_bytes) {
        Ok(value) => value,
        Err(chain::ChainError::TxTooLarge) => {
            return Err(SubmitTxError::InvalidArgument("tx too large".to_string()));
        }
        Err(chain::ChainError::DecodeFailed) => {
            return Err(SubmitTxError::InvalidArgument("decode failed".to_string()));
        }
        Err(chain::ChainError::TxAlreadySeen) => {
            return Err(SubmitTxError::Rejected("tx already seen".to_string()));
        }
        Err(chain::ChainError::InvalidFee) => {
            return Err(SubmitTxError::Rejected("invalid fee".to_string()));
        }
        Err(chain::ChainError::NonceConflict) => {
            return Err(SubmitTxError::Rejected("nonce conflict".to_string()));
        }
        Err(err) => {
            debug_print(format!("submit_ic_tx err: {:?}", err));
            return Err(SubmitTxError::Internal(format!("{:?}", err)));
        }
    };
    schedule_mining();
    Ok(tx_id.0.to_vec())
}

#[cfg(feature = "dev-faucet")]
#[ic_cdk::update]
fn dev_mint(address: Vec<u8>, amount: u128) {
    if address.len() != 20 {
        ic_cdk::trap("dev_mint: address must be 20 bytes");
    }
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&address);
    chain::dev_mint(addr, amount).unwrap_or_else(|_| ic_cdk::trap("dev_mint failed"));
}

#[ic_cdk::update]
fn produce_block(max_txs: u32) -> Result<ProduceBlockStatus, ProduceBlockError> {
    let limit = usize::try_from(max_txs).unwrap_or(0);
    match chain::produce_block(limit) {
        Ok(block) => {
            let gas_used = with_state(|state| {
                let mut total = 0u64;
                for tx_id in block.tx_ids.iter() {
                    if let Some(receipt) = state.receipts.get(tx_id) {
                        total = total.saturating_add(receipt.gas_used);
                    }
                }
                total
            });
            let dropped = with_state(|state| {
                let metrics = *state.metrics_state.get();
                let mut count = 0u64;
                for bucket in metrics.buckets.iter() {
                    if bucket.block_number == block.number {
                        count = bucket.drops;
                        break;
                    }
                }
                count
            });
            Ok(ProduceBlockStatus::Produced {
                block_number: block.number,
                txs: block.tx_ids.len().try_into().unwrap_or(u32::MAX),
                gas_used,
                dropped: dropped.try_into().unwrap_or(u32::MAX),
            })
        }
        Err(chain::ChainError::NoExecutableTx) | Err(chain::ChainError::QueueEmpty) => {
            Ok(ProduceBlockStatus::NoOp {
                reason: NoOpReason::NoExecutableTx,
            })
        }
        Err(chain::ChainError::InvalidLimit) => Err(ProduceBlockError::InvalidArgument(
            "max_txs must be > 0".to_string(),
        )),
        Err(err) => {
            debug_print(format!("produce_block err: {:?}", err));
            Err(ProduceBlockError::Internal(format!("{:?}", err)))
        }
    }
}

#[ic_cdk::query]
fn get_block(number: u64) -> Result<BlockView, LookupError> {
    let pruned_before = with_state(|state| state.prune_state.get().pruned_before());
    if let Some(pruned) = pruned_before {
        if number <= pruned {
            return Err(LookupError::Pruned {
                pruned_before_block: pruned,
            });
        }
    }
    chain::get_block(number)
        .map(block_to_view)
        .ok_or(LookupError::NotFound)
}

#[ic_cdk::query]
fn get_receipt(tx_id: Vec<u8>) -> Result<ReceiptView, LookupError> {
    if tx_id.len() != 32 {
        return Err(LookupError::NotFound);
    }
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&tx_id);
    if let Some(receipt) = chain::get_receipt(&TxId(buf)) {
        return Ok(receipt_to_view(receipt));
    }
    let pruned_before = with_state(|state| state.prune_state.get().pruned_before());
    let loc = chain::get_tx_loc(&TxId(buf));
    if let Some(loc) = loc {
        if loc.kind == TxLocKind::Queued {
            return Err(LookupError::Pending);
        }
        if loc.kind == TxLocKind::Included {
            if let Some(pruned) = pruned_before {
                if loc.block_number <= pruned {
                    return Err(LookupError::Pruned {
                        pruned_before_block: pruned,
                    });
                }
            }
        }
    }
    Err(LookupError::NotFound)
}

#[ic_cdk::query]
fn rpc_eth_chain_id() -> u64 {
    CHAIN_ID
}

#[ic_cdk::query]
fn rpc_eth_block_number() -> u64 {
    chain::get_head_number()
}

#[ic_cdk::query]
fn rpc_eth_get_block_by_number(number: u64, full_tx: bool) -> Option<EthBlockView> {
    let block = chain::get_block(number)?;
    let txs = if full_tx {
        let mut list = Vec::with_capacity(block.tx_ids.len());
        for tx_id in block.tx_ids.iter() {
            if let Some(view) = tx_to_view(*tx_id) {
                list.push(view);
            }
        }
        EthTxListView::Full(list)
    } else {
        EthTxListView::Hashes(block.tx_ids.iter().map(|id| id.0.to_vec()).collect())
    };
    Some(EthBlockView {
        number: block.number,
        parent_hash: block.parent_hash.to_vec(),
        block_hash: block.block_hash.to_vec(),
        timestamp: block.timestamp,
        txs,
        state_root: block.state_root.to_vec(),
    })
}

#[ic_cdk::query]
fn rpc_eth_get_transaction_by_hash(tx_hash: Vec<u8>) -> Option<EthTxView> {
    let tx_id = tx_id_from_bytes(tx_hash)?;
    tx_to_view(tx_id)
}

#[ic_cdk::query]
fn rpc_eth_get_transaction_receipt(tx_hash: Vec<u8>) -> Option<EthReceiptView> {
    let tx_id = tx_id_from_bytes(tx_hash)?;
    let receipt = chain::get_receipt(&tx_id)?;
    Some(receipt_to_eth_view(receipt))
}

#[ic_cdk::update]
fn rpc_eth_send_raw_transaction(raw_tx: Vec<u8>) -> Result<Vec<u8>, SubmitTxError> {
    let tx_id = match chain::submit_tx(TxKind::EthSigned, raw_tx) {
        Ok(value) => value,
        Err(chain::ChainError::TxTooLarge) => {
            return Err(SubmitTxError::InvalidArgument("tx too large".to_string()));
        }
        Err(chain::ChainError::DecodeFailed) => {
            return Err(SubmitTxError::InvalidArgument("decode failed".to_string()));
        }
        Err(chain::ChainError::TxAlreadySeen) => {
            return Err(SubmitTxError::Rejected("tx already seen".to_string()));
        }
        Err(chain::ChainError::InvalidFee) => {
            return Err(SubmitTxError::Rejected("invalid fee".to_string()));
        }
        Err(chain::ChainError::NonceConflict) => {
            return Err(SubmitTxError::Rejected("nonce conflict".to_string()));
        }
        Err(err) => {
            debug_print(format!("rpc_eth_send_raw_transaction err: {:?}", err));
            return Err(SubmitTxError::Internal(format!("{:?}", err)));
        }
    };
    schedule_mining();
    Ok(tx_id.0.to_vec())
}

#[ic_cdk::query]
fn get_cycle_balance() -> u128 {
    ic_cdk::api::canister_cycle_balance()
}

#[ic_cdk::query]
fn health() -> HealthView {
    with_state(|state| {
        let head = *state.head.get();
        let chain_state = *state.chain_state.get();
        let queue_len = state.pending_by_sender_nonce.len();
        HealthView {
            tip_number: head.number,
            tip_hash: head.block_hash.to_vec(),
            last_block_time: chain_state.last_block_time,
            queue_len,
            auto_mine_enabled: chain_state.auto_mine_enabled,
            is_producing: chain_state.is_producing,
            mining_scheduled: chain_state.mining_scheduled,
        }
    })
}

#[ic_cdk::query]
fn metrics(window: u64) -> MetricsView {
    let cycles = ic_cdk::api::canister_cycle_balance();
    with_state(|state| {
        let queue_len = state.pending_by_sender_nonce.len();
        let window = clamp_metrics_window(window);
        let metrics = *state.metrics_state.get();
        let summary = metrics.window_summary(window);
        let pruned_before_block = state.prune_state.get().pruned_before();
        let rate = summary.block_rate_per_sec_x1000();
        let avg = if summary.blocks == 0 {
            0
        } else {
            summary.txs / summary.blocks
        };
        MetricsView {
            window,
            blocks: summary.blocks,
            txs: summary.txs,
            avg_txs_per_block: avg,
            block_rate_per_sec_x1000: rate,
            ema_block_rate_per_sec_x1000: metrics.ema_block_rate_x1000,
            ema_txs_per_block_x1000: metrics.ema_txs_per_block_x1000,
            queue_len,
            drop_counts: collect_drop_counts(&metrics),
            total_submitted: metrics.total_submitted,
            total_included: metrics.total_included,
            total_dropped: metrics.total_dropped,
            cycles,
            pruned_before_block,
        }
    })
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

#[ic_cdk::update]
fn prune_blocks(retain: u64, max_ops: u32) -> Result<PruneResultView, ProduceBlockError> {
    match chain::prune_blocks(retain, max_ops) {
        Ok(result) => Ok(PruneResultView {
            did_work: result.did_work,
            remaining: result.remaining,
            pruned_before_block: result.pruned_before_block,
        }),
        Err(chain::ChainError::InvalidLimit) => Err(ProduceBlockError::InvalidArgument(
            "retain/max_ops must be > 0".to_string(),
        )),
        Err(err) => Err(ProduceBlockError::Internal(format!("{:?}", err))),
    }
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

fn clamp_metrics_window(window: u64) -> u64 {
    const DEFAULT_WINDOW: u64 = 128;
    const MAX_WINDOW: u64 = 2048;
    if window == 0 {
        return DEFAULT_WINDOW;
    }
    if window > MAX_WINDOW {
        return MAX_WINDOW;
    }
    window
}

fn collect_drop_counts(metrics: &evm_db::chain_data::MetricsStateV1) -> Vec<DropCountView> {
    metrics
        .drop_counts
        .iter()
        .enumerate()
        .filter_map(|(idx, count)| {
            if *count == 0 {
                None
            } else {
                Some(DropCountView {
                    code: idx as u16,
                    count: *count,
                })
            }
        })
        .collect()
}

fn tx_id_from_bytes(tx_id: Vec<u8>) -> Option<TxId> {
    if tx_id.len() != 32 {
        return None;
    }
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&tx_id);
    Some(TxId(buf))
}

fn tx_to_view(tx_id: TxId) -> Option<EthTxView> {
    let envelope = chain::get_tx_envelope(&tx_id)?;
    let (block_number, tx_index) = match chain::get_tx_loc(&tx_id) {
        Some(TxLoc {
            kind: TxLocKind::Included,
            block_number,
            tx_index,
            ..
        }) => (Some(block_number), Some(tx_index)),
        _ => (None, None),
    };
    envelope_to_eth_view(envelope, block_number, tx_index)
}

fn envelope_to_eth_view(
    envelope: TxEnvelope,
    block_number: Option<u64>,
    tx_index: Option<u32>,
) -> Option<EthTxView> {
    let caller = match envelope.kind {
        TxKind::IcSynthetic => envelope.caller_evm.unwrap_or([0u8; 20]),
        TxKind::EthSigned => [0u8; 20],
    };
    let decoded = if let Ok(decoded) =
        evm_core::tx_decode::decode_tx_view(envelope.kind, caller, &envelope.tx_bytes)
    {
        Some(DecodedTxView {
            from: decoded.from.to_vec(),
            to: decoded.to.map(|addr| addr.to_vec()),
            nonce: decoded.nonce,
            value: decoded.value.to_vec(),
            input: decoded.input,
            gas_limit: decoded.gas_limit,
            gas_price: decoded.gas_price,
            chain_id: decoded.chain_id,
        })
    } else {
        None
    };

    Some(EthTxView {
        hash: envelope.tx_id.0.to_vec(),
        kind: tx_kind_to_view(envelope.kind),
        raw: envelope.tx_bytes,
        decode_ok: decoded.is_some(),
        decoded,
        block_number,
        tx_index,
    })
}

fn receipt_to_eth_view(receipt: ReceiptLike) -> EthReceiptView {
    EthReceiptView {
        tx_hash: receipt.tx_id.0.to_vec(),
        block_number: receipt.block_number,
        tx_index: receipt.tx_index,
        status: receipt.status,
        gas_used: receipt.gas_used,
        effective_gas_price: receipt.effective_gas_price,
        contract_address: receipt.contract_address.map(|v| v.to_vec()),
        logs: receipt.logs.into_iter().map(log_to_view).collect(),
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
        ic_cdk_timers::set_timer(std::time::Duration::from_millis(interval_ms), async move {
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
        if state.ready_queue.len() == 0 {
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
ic_cdk::export_candid!();

#[cfg(test)]
mod tests {
    use super::{clamp_return_data, tx_id_from_bytes};
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

    #[test]
    fn tx_id_from_bytes_rejects_wrong_len() {
        let out = tx_id_from_bytes(vec![1u8; 31]);
        assert!(out.is_none());
    }

    #[test]
    fn tx_id_from_bytes_accepts_32() {
        let input = vec![9u8; 32];
        let out = tx_id_from_bytes(input.clone()).expect("tx_id");
        assert_eq!(out.0.to_vec(), input);
    }

}
