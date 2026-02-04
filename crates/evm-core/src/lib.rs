//! どこで: evm-coreの入口 / 何を: Phase1の実行・ブロック生成の核 / なぜ: canisterから分離するため

pub mod base_fee;
pub mod chain;
pub mod commit;
pub mod db_adapter;
pub mod export;
pub mod hash;
pub mod revm_db;
pub mod revm_exec;
pub mod selfdestruct;
pub mod state_root;
pub(crate) mod time;
pub mod tx_decode;
pub(crate) mod tx_decode_deposit;
pub mod tx_submit;
