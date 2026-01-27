//! どこで: evm-coreの入口 / 何を: Phase1の実行・ブロック生成の核 / なぜ: canisterから分離するため

pub mod chain;
pub mod commit;
pub mod db_adapter;
pub mod hash;
pub mod selfdestruct;
pub mod state_root;
