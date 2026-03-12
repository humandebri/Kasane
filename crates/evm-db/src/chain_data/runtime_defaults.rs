//! どこで: chain_data の実行時デフォルト / 何を: 運用調整しうる既定値を集約 / なぜ: 仕様固定値と責務分離するため

// unwrap dispatch の既定許可先。
// mainnet では同一 subnet 上の wrap_canister を固定で使う。
pub const DEFAULT_WRAP_CANISTER_ID_TEXT: &str = "lpuz5-uyaaa-aaaam-ah4da-cai";
// unwrap burn の既定 factory。
// precompile はこの factory 配下の wrapped token だけを正として burn する。
pub const DEFAULT_WRAP_FACTORY_ADDRESS: [u8; 20] = [
    0x90, 0x57, 0xeb, 0x7d, 0x90, 0x95, 0xe5, 0xe0, 0xff, 0x20, 0x91, 0xb8, 0x87, 0x0c, 0x75, 0x3f,
    0xb1, 0x6d, 0x3e, 0xbb,
];

// 自動ブロック生成の既定間隔（ms）
pub const DEFAULT_MINING_INTERVAL_MS: u64 = 2_000;

// ガス関連の既定値（Phase1の足場）
// block gas limit は固定運用。更新時は docs/ops/ic-wasm-workflow.md の
// staging計測手順（失敗ゼロ最大候補 + 20% headroom）で根拠を取ってから変更する。
// 運用方針: 1.00 gwei から引き上げ検討後、250 gwei を初期デフォルトに採用。
pub const DEFAULT_BASE_FEE: u64 = 250_000_000_000;
// 現行運用では legacy gas_price 下限と EIP-1559 priority fee 下限を同値で扱う。
pub const DEFAULT_MIN_FEE_FLOOR: u64 = 150_000_000_000;
pub const DEFAULT_BLOCK_GAS_LIMIT: u64 = 12_000_000;
// 0 の場合は命令数ベースの早期打ち切りを無効化する。
// 既定値は「上限手前で安全に止める」ための保守値。
pub const DEFAULT_INSTRUCTION_SOFT_LIMIT: u64 = 4_000_000_000;

// prune 実行の既定値
pub const DEFAULT_PRUNE_TIMER_INTERVAL_MS: u64 = 3_600_000;
pub const DEFAULT_PRUNE_MAX_OPS_PER_TICK: u32 = 5_000;

// prune 入力の安全ガード
pub const MIN_PRUNE_TIMER_INTERVAL_MS: u64 = 1_000;
pub const MIN_PRUNE_MAX_OPS_PER_TICK: u32 = 1;

// 無効デコード/署名スパム耐性の既定値
pub const DEFAULT_MAX_DECODE_DROPS_PER_BLOCK: usize = 128;
pub const DEFAULT_DECODE_SUPPRESS_STRIKES_PER_BLOCK: u16 = 8;
pub const DEFAULT_DECODE_SUPPRESS_WINDOW_SECS: u64 = 120;
pub const DEFAULT_MAX_DECODE_SUPPRESS_PRINCIPALS: usize = 4_096;
