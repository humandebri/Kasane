//! どこで: Stable Memoryの割当 / 何を: MemoryIdの凍結とMemoryManager初期化 / なぜ: レイアウトを固定するため

use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::DefaultMemoryImpl;
use ic_stable_structures::Memory;
use std::cell::RefCell;

pub type VMem = VirtualMemory<DefaultMemoryImpl>;
pub const WASM_PAGE_SIZE_BYTES: u64 = 65_536;

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
    BlobArena = 25,
    BlobArenaMeta = 26,
    BlobAllocTable = 27,
    BlobFreeList = 28,
    PruneJournal = 29,
    PruneConfig = 30,
    CorruptLog = 31,
    OpsConfig = 32,
    OpsState = 33,
    LogConfig = 34,
    SchemaMigrationState = 35,
    OpsMetrics = 36,
    TxLocsV3 = 37,
    DroppedRingState = 38,
    DroppedRing = 39,
    StateStorageRoots = 40,
    StateRootMeta = 41,
    StateRootMismatch = 42,
    StateRootMetrics = 43,
    StateRootMigration = 44,
    StateRootNodeDb = 45,
    StateRootAccountLeafHash = 46,
    StateRootGcQueue = 47,
    StateRootGcState = 48,
    PrincipalPendingCount = 49,
    PendingFeeIndex = 50,
    PendingFeeKeyByTxId = 51,
    ReadyBySeq = 52,
    EthTxHashIndex = 53,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryRegionInfo {
    pub id: AppMemoryId,
    pub name: &'static str,
    pub include_in_estimate: bool,
}

const ALL_MEMORY_REGIONS: [MemoryRegionInfo; 54] = [
    MemoryRegionInfo {
        id: AppMemoryId::Upgrades,
        name: "Upgrades",
        include_in_estimate: false,
    },
    MemoryRegionInfo {
        id: AppMemoryId::Meta,
        name: "Meta",
        include_in_estimate: false,
    },
    MemoryRegionInfo {
        id: AppMemoryId::Accounts,
        name: "Accounts",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::Storage,
        name: "Storage",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::Codes,
        name: "Codes",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::StateAux,
        name: "StateAux",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::QueueMeta,
        name: "QueueMeta",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::Queue,
        name: "Queue",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::SeenTx,
        name: "SeenTx",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::TxStore,
        name: "TxStore",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::TxIndex,
        name: "TxIndex",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::Receipts,
        name: "Receipts",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::Blocks,
        name: "Blocks",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::Head,
        name: "Head",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::ChainState,
        name: "ChainState",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::CallerNonces,
        name: "CallerNonces",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::TxLocs,
        name: "TxLocs",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::PruneState,
        name: "PruneState",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::ReadyQueue,
        name: "ReadyQueue",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::ReadyKeyByTxId,
        name: "ReadyKeyByTxId",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::PendingBySenderNonce,
        name: "PendingBySenderNonce",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::PendingMinNonce,
        name: "PendingMinNonce",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::PendingMetaByTxId,
        name: "PendingMetaByTxId",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::SenderExpectedNonce,
        name: "SenderExpectedNonce",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::PendingCurrentBySender,
        name: "PendingCurrentBySender",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::BlobArena,
        name: "BlobArena",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::BlobArenaMeta,
        name: "BlobArenaMeta",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::BlobAllocTable,
        name: "BlobAllocTable",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::BlobFreeList,
        name: "BlobFreeList",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::PruneJournal,
        name: "PruneJournal",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::PruneConfig,
        name: "PruneConfig",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::CorruptLog,
        name: "CorruptLog",
        include_in_estimate: false,
    },
    MemoryRegionInfo {
        id: AppMemoryId::OpsConfig,
        name: "OpsConfig",
        include_in_estimate: false,
    },
    MemoryRegionInfo {
        id: AppMemoryId::OpsState,
        name: "OpsState",
        include_in_estimate: false,
    },
    MemoryRegionInfo {
        id: AppMemoryId::LogConfig,
        name: "LogConfig",
        include_in_estimate: false,
    },
    MemoryRegionInfo {
        id: AppMemoryId::SchemaMigrationState,
        name: "SchemaMigrationState",
        include_in_estimate: false,
    },
    MemoryRegionInfo {
        id: AppMemoryId::OpsMetrics,
        name: "OpsMetrics",
        include_in_estimate: false,
    },
    MemoryRegionInfo {
        id: AppMemoryId::TxLocsV3,
        name: "TxLocsV3",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::DroppedRingState,
        name: "DroppedRingState",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::DroppedRing,
        name: "DroppedRing",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::StateStorageRoots,
        name: "StateStorageRoots",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::StateRootMeta,
        name: "StateRootMeta",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::StateRootMismatch,
        name: "StateRootMismatch",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::StateRootMetrics,
        name: "StateRootMetrics",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::StateRootMigration,
        name: "StateRootMigration",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::StateRootNodeDb,
        name: "StateRootNodeDb",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::StateRootAccountLeafHash,
        name: "StateRootAccountLeafHash",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::StateRootGcQueue,
        name: "StateRootGcQueue",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::StateRootGcState,
        name: "StateRootGcState",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::PrincipalPendingCount,
        name: "PrincipalPendingCount",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::PendingFeeIndex,
        name: "PendingFeeIndex",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::PendingFeeKeyByTxId,
        name: "PendingFeeKeyByTxId",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::ReadyBySeq,
        name: "ReadyBySeq",
        include_in_estimate: true,
    },
    MemoryRegionInfo {
        id: AppMemoryId::EthTxHashIndex,
        name: "EthTxHashIndex",
        include_in_estimate: true,
    },
];

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
            AppMemoryId::BlobArena => 25,
            AppMemoryId::BlobArenaMeta => 26,
            AppMemoryId::BlobAllocTable => 27,
            AppMemoryId::BlobFreeList => 28,
            AppMemoryId::PruneJournal => 29,
            AppMemoryId::PruneConfig => 30,
            AppMemoryId::CorruptLog => 31,
            AppMemoryId::OpsConfig => 32,
            AppMemoryId::OpsState => 33,
            AppMemoryId::LogConfig => 34,
            AppMemoryId::SchemaMigrationState => 35,
            AppMemoryId::OpsMetrics => 36,
            AppMemoryId::TxLocsV3 => 37,
            AppMemoryId::DroppedRingState => 38,
            AppMemoryId::DroppedRing => 39,
            AppMemoryId::StateStorageRoots => 40,
            AppMemoryId::StateRootMeta => 41,
            AppMemoryId::StateRootMismatch => 42,
            AppMemoryId::StateRootMetrics => 43,
            AppMemoryId::StateRootMigration => 44,
            AppMemoryId::StateRootNodeDb => 45,
            AppMemoryId::StateRootAccountLeafHash => 46,
            AppMemoryId::StateRootGcQueue => 47,
            AppMemoryId::StateRootGcState => 48,
            AppMemoryId::PrincipalPendingCount => 49,
            AppMemoryId::PendingFeeIndex => 50,
            AppMemoryId::PendingFeeKeyByTxId => 51,
            AppMemoryId::ReadyBySeq => 52,
            AppMemoryId::EthTxHashIndex => 53,
        }
    }

    pub fn as_memory_id(self) -> MemoryId {
        MemoryId::new(self.as_u8())
    }
}

pub fn all_memory_regions() -> &'static [MemoryRegionInfo] {
    &ALL_MEMORY_REGIONS
}

pub fn chain_data_memory_ids_for_estimate() -> Vec<AppMemoryId> {
    all_memory_regions()
        .iter()
        .filter(|region| region.include_in_estimate && !is_blob_store_region(region.id))
        .map(|region| region.id)
        .collect()
}

pub fn is_blob_store_region(id: AppMemoryId) -> bool {
    matches!(
        id,
        AppMemoryId::BlobArena
            | AppMemoryId::BlobArenaMeta
            | AppMemoryId::BlobAllocTable
            | AppMemoryId::BlobFreeList
    )
}

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
}

pub fn get_memory(id: AppMemoryId) -> VMem {
    MEMORY_MANAGER.with(|m| m.borrow().get(id.as_memory_id()))
}

pub fn memory_size_pages(id: AppMemoryId) -> u64 {
    get_memory(id).size()
}
