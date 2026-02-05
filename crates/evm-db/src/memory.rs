//! どこで: Stable Memoryの割当 / 何を: MemoryIdの凍結とMemoryManager初期化 / なぜ: レイアウトを固定するため

use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::DefaultMemoryImpl;
use std::cell::RefCell;

pub type VMem = VirtualMemory<DefaultMemoryImpl>;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppMemoryId {
    Upgrades = 0,
    Meta = 1,
    Accounts = 2,
    Storage = 3,
    Codes = 4,
    StateAux = 5,
    QueueMeta = 6,
    Queue = 7,
    SeenTx = 8,
    TxStore = 9,
    TxIndex = 10,
    Receipts = 11,
    Blocks = 12,
    Head = 13,
    ChainState = 14,
    CallerNonces = 15,
    TxLocs = 16,
    PruneState = 17,
    ReadyQueue = 18,
    ReadyKeyByTxId = 19,
    PendingBySenderNonce = 20,
    PendingMinNonce = 21,
    PendingMetaByTxId = 22,
    SenderExpectedNonce = 23,
    PendingCurrentBySender = 24,
    BlocksPtr = 25,
    ReceiptsPtr = 26,
    TxIndexPtr = 27,
    BlobArena = 28,
    BlobArenaMeta = 29,
    BlobAllocTable = 30,
    BlobFreeList = 31,
    PruneJournal = 32,
    PruneConfig = 33,
    CorruptLog = 34,
    OpsConfig = 35,
    OpsState = 36,
    Reserved37 = 37,
    Reserved38 = 38,
    OpsMetrics = 39,
    Reserved40 = 40,
    MinerAllowlist = 41,
    DroppedRingState = 42,
    DroppedRing = 43,
    StateStorageRoots = 44,
    StateRootMeta = 45,
    StateRootMismatch = 46,
    StateRootMetrics = 47,
    StateRootMigration = 48,
    StateRootNodeDb = 49,
    StateRootAccountLeafHash = 50,
    StateRootGcQueue = 51,
    StateRootGcState = 52,
}

impl AppMemoryId {
    pub fn as_u8(self) -> u8 {
        match self {
            AppMemoryId::Upgrades => 0,
            AppMemoryId::Meta => 1,
            AppMemoryId::Accounts => 2,
            AppMemoryId::Storage => 3,
            AppMemoryId::Codes => 4,
            AppMemoryId::StateAux => 5,
            AppMemoryId::QueueMeta => 6,
            AppMemoryId::Queue => 7,
            AppMemoryId::SeenTx => 8,
            AppMemoryId::TxStore => 9,
            AppMemoryId::TxIndex => 10,
            AppMemoryId::Receipts => 11,
            AppMemoryId::Blocks => 12,
            AppMemoryId::Head => 13,
            AppMemoryId::ChainState => 14,
            AppMemoryId::CallerNonces => 15,
            AppMemoryId::TxLocs => 16,
            AppMemoryId::PruneState => 17,
            AppMemoryId::ReadyQueue => 18,
            AppMemoryId::ReadyKeyByTxId => 19,
            AppMemoryId::PendingBySenderNonce => 20,
            AppMemoryId::PendingMinNonce => 21,
            AppMemoryId::PendingMetaByTxId => 22,
            AppMemoryId::SenderExpectedNonce => 23,
            AppMemoryId::PendingCurrentBySender => 24,
            AppMemoryId::BlocksPtr => 25,
            AppMemoryId::ReceiptsPtr => 26,
            AppMemoryId::TxIndexPtr => 27,
            AppMemoryId::BlobArena => 28,
            AppMemoryId::BlobArenaMeta => 29,
            AppMemoryId::BlobAllocTable => 30,
            AppMemoryId::BlobFreeList => 31,
            AppMemoryId::PruneJournal => 32,
            AppMemoryId::PruneConfig => 33,
            AppMemoryId::CorruptLog => 34,
            AppMemoryId::OpsConfig => 35,
            AppMemoryId::OpsState => 36,
            AppMemoryId::Reserved37 => 37,
            AppMemoryId::Reserved38 => 38,
            AppMemoryId::OpsMetrics => 39,
            AppMemoryId::Reserved40 => 40,
            AppMemoryId::MinerAllowlist => 41,
            AppMemoryId::DroppedRingState => 42,
            AppMemoryId::DroppedRing => 43,
            AppMemoryId::StateStorageRoots => 44,
            AppMemoryId::StateRootMeta => 45,
            AppMemoryId::StateRootMismatch => 46,
            AppMemoryId::StateRootMetrics => 47,
            AppMemoryId::StateRootMigration => 48,
            AppMemoryId::StateRootNodeDb => 49,
            AppMemoryId::StateRootAccountLeafHash => 50,
            AppMemoryId::StateRootGcQueue => 51,
            AppMemoryId::StateRootGcState => 52,
        }
    }

    pub fn as_memory_id(self) -> MemoryId {
        MemoryId::new(self.as_u8())
    }
}

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
}

pub fn get_memory(id: AppMemoryId) -> VMem {
    MEMORY_MANAGER.with(|m| m.borrow().get(id.as_memory_id()))
}
