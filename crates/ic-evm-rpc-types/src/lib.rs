//! どこで: canister内の共有DTO層 / 何を: Candid公開型を集約 / なぜ: wrapper分割時もAPI互換を保つため

use candid::CandidType;
use serde::Deserialize;

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
    NoOp { reason: NoOpReason },
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum NoOpReason {
    NoExecutableTx,
    CycleCritical,
    NeedsMigration,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ProduceBlockError {
    Internal(String),
    InvalidArgument(String),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct OpsConfigView {
    pub low_watermark: u128,
    pub critical: u128,
    pub freeze_on_critical: bool,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum OpsModeView {
    Normal,
    Low,
    Critical,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct OpsStatusView {
    pub config: OpsConfigView,
    pub last_cycle_balance: u128,
    pub last_check_ts: u64,
    pub mode: OpsModeView,
    pub safe_stop_latched: bool,
    pub needs_migration: bool,
    pub schema_version: u32,
    pub log_filter_override: Option<String>,
    pub log_truncated_count: u64,
    pub critical_corrupt: bool,
    pub mining_error_count: u64,
    pub prune_error_count: u64,
    pub decode_failure_count: u64,
    pub decode_failure_last_ts: u64,
    pub decode_failure_last_label: Option<String>,
    pub block_gas_limit: u64,
    pub instruction_soft_limit: u64,
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
    pub l1_data_fee: u128,
    pub operator_fee: u128,
    pub total_fee: u128,
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
    pub block_gas_limit: u64,
    pub instruction_soft_limit: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
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
    pub eth_tx_hash: Option<Vec<u8>>,
    pub caller_principal: Option<Vec<u8>>,
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
    pub eth_tx_hash: Option<Vec<u8>>,
    pub block_number: u64,
    pub tx_index: u32,
    pub status: u8,
    pub gas_used: u64,
    pub effective_gas_price: u64,
    pub l1_data_fee: u128,
    pub operator_fee: u128,
    pub total_fee: u128,
    pub contract_address: Option<Vec<u8>>,
    pub logs: Vec<EthReceiptLogView>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct EthReceiptLogView {
    pub address: Vec<u8>,
    pub topics: Vec<Vec<u8>>,
    pub data: Vec<u8>,
    pub log_index: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct EthLogFilterView {
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
    pub address: Option<Vec<u8>>,
    pub topic0: Option<Vec<u8>>,
    pub topic1: Option<Vec<u8>>,
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct EthLogItemView {
    pub block_number: u64,
    pub tx_index: u32,
    pub log_index: u32,
    pub tx_hash: Vec<u8>,
    pub eth_tx_hash: Option<Vec<u8>>,
    pub address: Vec<u8>,
    pub topics: Vec<Vec<u8>>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct EthLogsCursorView {
    pub block_number: u64,
    pub tx_index: u32,
    pub log_index: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct EthLogsPageView {
    pub items: Vec<EthLogItemView>,
    pub next_cursor: Option<EthLogsCursorView>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct RpcAccessListItemView {
    pub address: Vec<u8>,
    pub storage_keys: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct RpcCallObjectView {
    pub to: Option<Vec<u8>>,
    pub from: Option<Vec<u8>>,
    pub gas: Option<u64>,
    pub gas_price: Option<u128>,
    pub nonce: Option<u64>,
    pub max_fee_per_gas: Option<u128>,
    pub max_priority_fee_per_gas: Option<u128>,
    pub chain_id: Option<u64>,
    pub tx_type: Option<u64>,
    pub access_list: Option<Vec<RpcAccessListItemView>>,
    pub value: Option<Vec<u8>>,
    pub data: Option<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct RpcCallResultView {
    pub status: u8,
    pub gas_used: u64,
    pub return_data: Vec<u8>,
    pub revert_data: Option<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct RpcErrorView {
    pub code: u32,
    pub message: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub enum GetLogsErrorView {
    RangeTooLarge,
    TooManyResults,
    UnsupportedFilter(String),
    InvalidArgument(String),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum RpcBlockLookupView {
    Found(EthBlockView),
    Pruned { pruned_before_block: u64 },
    NotFound,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum RpcReceiptLookupView {
    Found(EthReceiptView),
    Pruned { pruned_before_block: u64 },
    PossiblyPruned { pruned_before_block: u64 },
    NotFound,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ExportCursorView {
    pub block_number: u64,
    pub segment: u8,
    pub byte_offset: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ExportChunkView {
    pub segment: u8,
    pub start: u32,
    pub bytes: Vec<u8>,
    pub payload_len: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ExportResponseView {
    pub chunks: Vec<ExportChunkView>,
    pub next_cursor: Option<ExportCursorView>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ExportErrorView {
    InvalidCursor { message: String },
    Pruned { pruned_before_block: u64 },
    MissingData { message: String },
    Limit,
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

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct PrunePolicyView {
    pub target_bytes: u64,
    pub retain_days: u64,
    pub retain_blocks: u64,
    pub headroom_ratio_bps: u32,
    pub hard_emergency_ratio_bps: u32,
    pub timer_interval_ms: u64,
    pub max_ops_per_tick: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct PruneStatusView {
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

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct GenesisBalanceView {
    pub address: Vec<u8>,
    pub amount: u128,
}
