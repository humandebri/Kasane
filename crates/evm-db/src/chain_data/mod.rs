//! どこで: Phase1型の集約 / 何を: Tx/Block/Receiptの公開 / なぜ: 依存の簡略化

pub mod block;
pub mod caller;
pub mod chain_state;
pub mod constants;
pub mod metrics;
pub mod prune_state;
pub mod queue;
pub mod receipt;
pub mod tx_loc;
pub mod tx;

pub use block::{BlockData, Head};
pub use caller::CallerKey;
pub use chain_state::ChainStateV1;
pub use constants::{
    CALLER_KEY_LEN, CHAIN_STATE_SIZE_U32, HASH_LEN, MAX_PRINCIPAL_LEN, MAX_TXS_PER_BLOCK,
    MAX_TX_SIZE, RECEIPT_CONTRACT_ADDR_LEN, TX_ID_LEN,
};
pub use metrics::{MetricsStateV1, MetricsWindowSummary, METRICS_BUCKETS};
pub use prune_state::PruneStateV1;
pub use queue::QueueMeta;
pub use receipt::ReceiptLike;
pub use tx_loc::{TxLoc, TxLocKind};
pub use tx::{TxEnvelope, TxId, TxIndexEntry, TxKind};
