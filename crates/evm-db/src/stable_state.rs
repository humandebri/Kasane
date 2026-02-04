//! どこで: StableBTreeMapの結線 / 何を: accounts/storage/codesの初期化 / なぜ: MemoryId凍結を反映するため

use crate::blob_ptr::BlobPtr;
use crate::blob_store::BlobStore;
use crate::memory::{get_memory, AppMemoryId, VMem};
use crate::chain_data::{
    CallerKey, ChainStateV1, Head, L1BlockInfoParamsV1, L1BlockInfoSnapshotV1, MetricsStateV1,
    OpsConfigV1, OpsMetricsV1, OpsStateV1, PruneConfigV1, SystemTxHealthV1,
    PruneJournal, PruneStateV1, QueueMeta, SenderKey, SenderNonceKey, StoredTxBytes, TxId,
    ReadyKey,
};
use crate::chain_data::constants::CHAIN_ID;
use crate::types::keys::{AccountKey, CodeKey, StorageKey};
use crate::types::values::{AccountVal, CodeVal, U256Val};
use ic_stable_structures::{StableBTreeMap, StableCell};
use std::cell::RefCell;

pub type Accounts = StableBTreeMap<AccountKey, AccountVal, VMem>;
pub type Storage = StableBTreeMap<StorageKey, U256Val, VMem>;
pub type Codes = StableBTreeMap<CodeKey, CodeVal, VMem>;
pub type Queue = StableBTreeMap<u64, TxId, VMem>;
pub type SeenTx = StableBTreeMap<TxId, u8, VMem>;
pub type TxStore = StableBTreeMap<TxId, StoredTxBytes, VMem>;
pub type TxIndex = StableBTreeMap<TxId, BlobPtr, VMem>;
pub type Receipts = StableBTreeMap<TxId, BlobPtr, VMem>;
pub type Blocks = StableBTreeMap<u64, BlobPtr, VMem>;
pub type CallerNonces = StableBTreeMap<CallerKey, u64, VMem>;
pub type TxLocs = StableBTreeMap<TxId, crate::chain_data::TxLoc, VMem>;
pub type ReadyQueue = StableBTreeMap<ReadyKey, TxId, VMem>;
pub type ReadyKeyByTxId = StableBTreeMap<TxId, ReadyKey, VMem>;
pub type PendingBySenderNonce = StableBTreeMap<SenderNonceKey, TxId, VMem>;
pub type PendingMinNonce = StableBTreeMap<SenderKey, u64, VMem>;
pub type PendingMetaByTxId = StableBTreeMap<TxId, SenderNonceKey, VMem>;
pub type SenderExpectedNonce = StableBTreeMap<SenderKey, u64, VMem>;
pub type PendingCurrentBySender = StableBTreeMap<SenderKey, TxId, VMem>;
pub type PruneJournalMap = StableBTreeMap<u64, PruneJournal, VMem>;

pub struct StableState {
    pub accounts: Accounts,
    pub storage: Storage,
    pub codes: Codes,
    pub queue: Queue,
    pub seen_tx: SeenTx,
    pub tx_store: TxStore,
    pub tx_index: TxIndex,
    pub receipts: Receipts,
    pub blocks: Blocks,
    pub blob_store: BlobStore,
    pub queue_meta: StableCell<QueueMeta, VMem>,
    pub head: StableCell<Head, VMem>,
    pub chain_state: StableCell<ChainStateV1, VMem>,
    pub metrics_state: StableCell<MetricsStateV1, VMem>,
    pub prune_state: StableCell<PruneStateV1, VMem>,
    pub prune_config: StableCell<PruneConfigV1, VMem>,
    pub ops_config: StableCell<OpsConfigV1, VMem>,
    pub ops_state: StableCell<OpsStateV1, VMem>,
    pub ops_metrics: StableCell<OpsMetricsV1, VMem>,
    pub system_tx_health: StableCell<SystemTxHealthV1, VMem>,
    pub l1_block_info_params: StableCell<L1BlockInfoParamsV1, VMem>,
    pub l1_block_info_snapshot: StableCell<L1BlockInfoSnapshotV1, VMem>,
    pub prune_journal: PruneJournalMap,
    pub caller_nonces: CallerNonces,
    pub tx_locs: TxLocs,
    pub ready_queue: ReadyQueue,
    pub ready_key_by_tx_id: ReadyKeyByTxId,
    pub pending_by_sender_nonce: PendingBySenderNonce,
    pub pending_min_nonce: PendingMinNonce,
    pub pending_meta_by_tx_id: PendingMetaByTxId,
    pub sender_expected_nonce: SenderExpectedNonce,
    pub pending_current_by_sender: PendingCurrentBySender,
}

thread_local! {
    static STABLE_STATE: RefCell<Option<StableState>> = const { RefCell::new(None) };
}

pub fn init_stable_state() {
    let accounts = StableBTreeMap::init(get_memory(AppMemoryId::Accounts));
    let storage = StableBTreeMap::init(get_memory(AppMemoryId::Storage));
    let codes = StableBTreeMap::init(get_memory(AppMemoryId::Codes));
    let queue = StableBTreeMap::init(get_memory(AppMemoryId::Queue));
    let seen_tx = StableBTreeMap::init(get_memory(AppMemoryId::SeenTx));
    let tx_store = StableBTreeMap::init(get_memory(AppMemoryId::TxStore));
    let tx_index = StableBTreeMap::init(get_memory(AppMemoryId::TxIndexPtr));
    let receipts = StableBTreeMap::init(get_memory(AppMemoryId::ReceiptsPtr));
    let blocks = StableBTreeMap::init(get_memory(AppMemoryId::BlocksPtr));
    let blob_store = BlobStore::new(
        get_memory(AppMemoryId::BlobArena),
        StableCell::init(get_memory(AppMemoryId::BlobArenaMeta), 0u64),
        StableBTreeMap::init(get_memory(AppMemoryId::BlobAllocTable)),
        StableBTreeMap::init(get_memory(AppMemoryId::BlobFreeList)),
    );
    let queue_meta = StableCell::init(get_memory(AppMemoryId::QueueMeta), QueueMeta::new());
    let head = StableCell::init(
        get_memory(AppMemoryId::Head),
        Head {
            number: 0,
            block_hash: [0u8; 32],
            timestamp: 0,
        },
    );
    let chain_state = StableCell::init(
        get_memory(AppMemoryId::ChainState),
        ChainStateV1::new(CHAIN_ID),
    );
    let metrics_state = StableCell::init(get_memory(AppMemoryId::StateAux), MetricsStateV1::new());
    let prune_state = StableCell::init(get_memory(AppMemoryId::PruneState), PruneStateV1::new());
    let prune_config = StableCell::init(get_memory(AppMemoryId::PruneConfig), PruneConfigV1::new());
    let ops_config = StableCell::init(get_memory(AppMemoryId::OpsConfig), OpsConfigV1::new());
    let ops_state = StableCell::init(get_memory(AppMemoryId::OpsState), OpsStateV1::new());
    let ops_metrics = StableCell::init(get_memory(AppMemoryId::OpsMetrics), OpsMetricsV1::new());
    let system_tx_health = StableCell::init(
        get_memory(AppMemoryId::SystemTxHealth),
        SystemTxHealthV1::new(),
    );
    let l1_block_info_params = StableCell::init(
        get_memory(AppMemoryId::L1BlockInfoParams),
        L1BlockInfoParamsV1::new(),
    );
    let l1_block_info_snapshot = StableCell::init(
        get_memory(AppMemoryId::L1BlockInfoSnapshot),
        L1BlockInfoSnapshotV1::new(),
    );
    let prune_journal = StableBTreeMap::init(get_memory(AppMemoryId::PruneJournal));
    let caller_nonces = StableBTreeMap::init(get_memory(AppMemoryId::CallerNonces));
    let tx_locs = StableBTreeMap::init(get_memory(AppMemoryId::TxLocs));
    let ready_queue = StableBTreeMap::init(get_memory(AppMemoryId::ReadyQueue));
    let ready_key_by_tx_id = StableBTreeMap::init(get_memory(AppMemoryId::ReadyKeyByTxId));
    let pending_by_sender_nonce =
        StableBTreeMap::init(get_memory(AppMemoryId::PendingBySenderNonce));
    let pending_min_nonce = StableBTreeMap::init(get_memory(AppMemoryId::PendingMinNonce));
    let pending_meta_by_tx_id = StableBTreeMap::init(get_memory(AppMemoryId::PendingMetaByTxId));
    let sender_expected_nonce = StableBTreeMap::init(get_memory(AppMemoryId::SenderExpectedNonce));
    let pending_current_by_sender =
        StableBTreeMap::init(get_memory(AppMemoryId::PendingCurrentBySender));
    STABLE_STATE.with(|s| {
        *s.borrow_mut() = Some(StableState {
            accounts,
            storage,
            codes,
            queue,
            seen_tx,
            tx_store,
            tx_index,
            receipts,
            blocks,
            blob_store,
            queue_meta,
            head,
            chain_state,
            metrics_state,
            prune_state,
            prune_config,
            ops_config,
            ops_state,
            ops_metrics,
            system_tx_health,
            l1_block_info_params,
            l1_block_info_snapshot,
            prune_journal,
            caller_nonces,
            tx_locs,
            ready_queue,
            ready_key_by_tx_id,
            pending_by_sender_nonce,
            pending_min_nonce,
            pending_meta_by_tx_id,
            sender_expected_nonce,
            pending_current_by_sender,
        });
    });
}

pub fn with_state<R>(f: impl FnOnce(&StableState) -> R) -> R {
    STABLE_STATE.with(|s| {
        let borrowed = s.borrow();
        let state = borrowed
            .as_ref()
            .unwrap_or_else(|| ic_cdk::trap("stable_state: not initialized"));
        f(state)
    })
}

pub fn with_state_mut<R>(f: impl FnOnce(&mut StableState) -> R) -> R {
    STABLE_STATE.with(|s| {
        let mut borrowed = s.borrow_mut();
        let state = borrowed
            .as_mut()
            .unwrap_or_else(|| ic_cdk::trap("stable_state: not initialized"));
        f(state)
    })
}
