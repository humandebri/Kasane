//! どこで: evm-coreの入口 / 何を: Phase1の実行・ブロック生成の核 / なぜ: canisterから分離するため

pub mod base_fee;
pub(crate) mod bytes;
pub mod chain;
pub mod commit;
pub(crate) mod constants;
pub mod db_adapter;
pub mod export;
pub mod hash;
pub mod revm_db;
pub mod revm_exec;
pub mod selfdestruct;
pub mod state_root;
pub(crate) mod time;
pub(crate) mod trie_commit;
pub mod tx_decode;
pub mod tx_submit;

pub fn fee_recipient() -> [u8; 20] {
    constants::FEE_RECIPIENT.into_array()
}
