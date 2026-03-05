//! Phase0の土台（どこで: canister入口 / 何を: 初期化とupgradeフック / なぜ: Stable Memory凍結を守るため）

pub mod blob_ptr;
pub mod blob_store;
pub mod chain_data;
pub mod corrupt_log;
pub mod decode;
pub mod memory;
pub mod meta;
pub mod overlay;
pub mod size_class;
pub mod stable_state;
pub mod types;
pub mod upgrade;

// 依存境界の一本化: 上位クレートはic-stable-structuresを直接参照せず、
// evm-db経由でtrait/typeを使う。
pub use ic_stable_structures::storable::Bound;
pub use ic_stable_structures::Memory;
pub use ic_stable_structures::Storable;
