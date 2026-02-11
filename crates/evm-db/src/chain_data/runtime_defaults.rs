//! どこで: chain_data の実行時デフォルト / 何を: 運用調整しうる既定値を集約 / なぜ: 仕様固定値と責務分離するため

// 自動ブロック生成の既定間隔（ms）
pub const DEFAULT_MINING_INTERVAL_MS: u64 = 5_000;

// ガス関連の既定値（Phase1の足場）
// block gas limit は固定運用。更新時は docs/ops/ic-wasm-workflow.md の
// staging計測手順（失敗ゼロ最大候補 + 20% headroom）で根拠を取ってから変更する。
pub const DEFAULT_BASE_FEE: u64 = 1_000_000_000;
// legacy Tx の最低 gas_price。0 だと legacy 下限チェックが実質無効になるため非0を維持する。
pub const DEFAULT_MIN_GAS_PRICE: u64 = 1_000_000_000;
pub const DEFAULT_MIN_PRIORITY_FEE: u64 = 1_000_000_000;
pub const DEFAULT_BLOCK_GAS_LIMIT: u64 = 3_000_000;
// 0 の場合は命令数ベースの早期打ち切りを無効化する。
// 既定値は「上限手前で安全に止める」ための保守値。
pub const DEFAULT_INSTRUCTION_SOFT_LIMIT: u64 = 4_000_000_000;

// prune 実行の既定値
pub const DEFAULT_PRUNE_TIMER_INTERVAL_MS: u64 = 3_600_000;
pub const DEFAULT_PRUNE_MAX_OPS_PER_TICK: u32 = 5_000;

// prune 入力の安全ガード
pub const MIN_PRUNE_TIMER_INTERVAL_MS: u64 = 1_000;
pub const MIN_PRUNE_MAX_OPS_PER_TICK: u32 = 1;
