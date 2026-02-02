//! Phase0の土台（どこで: canister入口 / 何を: 初期化とupgradeフック / なぜ: Stable Memory凍結を守るため）

pub mod memory;
pub mod decode;
pub mod meta;
pub mod overlay;
pub mod blob_ptr;
pub mod blob_store;
pub mod size_class;
pub mod chain_data;
pub mod stable_state;
pub mod types;
pub mod upgrade;
