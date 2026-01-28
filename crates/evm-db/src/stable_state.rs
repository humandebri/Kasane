//! どこで: StableBTreeMapの結線 / 何を: accounts/storage/codesの初期化 / なぜ: MemoryId凍結を反映するため

use crate::memory::{get_memory, AppMemoryId, VMem};
use crate::chain_data::{
    BlockData, CallerKey, ChainStateV1, Head, QueueMeta, ReceiptLike, TxEnvelope, TxId,
    TxIndexEntry,
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
pub type TxStore = StableBTreeMap<TxId, TxEnvelope, VMem>;
pub type TxIndex = StableBTreeMap<TxId, TxIndexEntry, VMem>;
pub type Receipts = StableBTreeMap<TxId, ReceiptLike, VMem>;
pub type Blocks = StableBTreeMap<u64, BlockData, VMem>;
pub type CallerNonces = StableBTreeMap<CallerKey, u64, VMem>;
pub type TxLocs = StableBTreeMap<TxId, crate::chain_data::TxLoc, VMem>;

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
    pub queue_meta: StableCell<QueueMeta, VMem>,
    pub head: StableCell<Head, VMem>,
    pub chain_state: StableCell<ChainStateV1, VMem>,
    pub caller_nonces: CallerNonces,
    pub tx_locs: TxLocs,
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
    let tx_index = StableBTreeMap::init(get_memory(AppMemoryId::TxIndex));
    let receipts = StableBTreeMap::init(get_memory(AppMemoryId::Receipts));
    let blocks = StableBTreeMap::init(get_memory(AppMemoryId::Blocks));
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
    let caller_nonces = StableBTreeMap::init(get_memory(AppMemoryId::CallerNonces));
    let tx_locs = StableBTreeMap::init(get_memory(AppMemoryId::TxLocs));
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
            queue_meta,
            head,
            chain_state,
            caller_nonces,
            tx_locs,
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
