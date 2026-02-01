//! どこで: evm-coreの入口 / 何を: Phase1の実行・ブロック生成の核 / なぜ: canisterから分離するため

pub mod chain;
pub mod commit;
pub mod base_fee;
pub mod db_adapter;
pub mod hash;
pub mod selfdestruct;
pub mod state_root;
pub mod revm_db;
pub mod revm_exec;
pub mod tx_decode;
pub mod tx_submit;
