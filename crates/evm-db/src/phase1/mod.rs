//! どこで: Phase1型の集約 / 何を: Tx/Block/Receiptの公開 / なぜ: 依存の簡略化

pub mod block;
pub mod constants;
pub mod queue;
pub mod receipt;
pub mod tx;

pub use block::{BlockData, Head};
pub use constants::{HASH_LEN, MAX_TXS_PER_BLOCK, MAX_TX_SIZE, RECEIPT_CONTRACT_ADDR_LEN, TX_ID_LEN};
pub use queue::QueueMeta;
pub use receipt::ReceiptLike;
pub use tx::{TxEnvelope, TxId, TxIndexEntry, TxKind};
