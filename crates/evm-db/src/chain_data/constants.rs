//! どこで: Phase1の定数 / 何を: 互換性境界の凍結 / なぜ: 仕様の再現性を守るため

pub const TX_ID_LEN: usize = 32;
pub const TX_ID_LEN_U32: u32 = 32;
pub const HASH_LEN: usize = 32;
pub const HASH_LEN_U32: u32 = 32;

// Phase1のPoCとしての上限（必要ならPhase2で調整）
pub const MAX_TX_SIZE: usize = 128 * 1024;
pub const MAX_TX_SIZE_U32: u32 = 131_072;
pub const MAX_TXS_PER_BLOCK: usize = 1024;
pub const MAX_TXS_PER_BLOCK_U32: u32 = 1024;
pub const CHAIN_ID: u64 = 4_801_360;

pub const RECEIPT_CONTRACT_ADDR_LEN: usize = 20;
pub const RECEIPT_CONTRACT_ADDR_LEN_U32: u32 = 20;

// ExecResultの返却サイズ制限（HTTP/RPCでの応答肥大化を防ぐ）
pub const MAX_RETURN_DATA: usize = 32 * 1024;

// Principalは最大29bytesなので、長さ1byte + 29bytesで固定化する
pub const MAX_PRINCIPAL_LEN: usize = 29;
pub const CALLER_KEY_LEN: usize = 30;

// StableCellの固定長ヘッダ
pub const CHAIN_STATE_SIZE_U32: u32 = 72;

// 自動ブロック生成の既定間隔（ms）
pub const DEFAULT_MINING_INTERVAL_MS: u64 = 5_000;

// ガス関連の既定値（Phase1の足場）
pub const DEFAULT_BASE_FEE: u64 = 1_000_000_000;
pub const DEFAULT_MIN_GAS_PRICE: u64 = 0;
pub const DEFAULT_MIN_PRIORITY_FEE: u64 = 1_000_000_000;
pub const DEFAULT_BLOCK_GAS_LIMIT: u64 = 15_000_000;
pub const ELASTICITY_MULTIPLIER: u64 = 2;
pub const BASE_FEE_MAX_CHANGE_DENOMINATOR: u64 = 8;

// TxLocの固定長
pub const TX_LOC_SIZE_U32: u32 = 24;

// TxLoc の drop_code（最低限の分類）
pub const DROP_CODE_DECODE: u16 = 1;
pub const DROP_CODE_EXEC: u16 = 2;
pub const DROP_CODE_MISSING: u16 = 3;
pub const DROP_CODE_CALLER_MISSING: u16 = 4;
pub const DROP_CODE_INVALID_FEE: u16 = 5;

// logs/receiptの上限
pub const MAX_LOGS_PER_TX: usize = 64;
pub const MAX_LOG_TOPICS: usize = 4;
pub const MAX_LOG_DATA: usize = 4096;
pub const MAX_LOG_DATA_U32: u32 = 4096;

// Receiptの最大サイズ（固定部 + 可変部の上限）
pub const RECEIPT_MAX_SIZE_U32: u32 =
    32 + 8 + 4 + 1 + 8 + 8 + 4 + (MAX_RETURN_DATA as u32) + 1 + RECEIPT_CONTRACT_ADDR_LEN_U32
        + 4
        + (MAX_LOGS_PER_TX as u32)
            * (20 + 4 + (MAX_LOG_TOPICS as u32) * 32 + 4 + MAX_LOG_DATA_U32);

pub const BLOCK_BASE_SIZE_U32: u32 = 8 + HASH_LEN_U32 + HASH_LEN_U32 + 8 + HASH_LEN_U32 + HASH_LEN_U32 + 4;
pub const MAX_BLOCK_DATA_SIZE_U32: u32 = BLOCK_BASE_SIZE_U32 + (HASH_LEN_U32 * MAX_TXS_PER_BLOCK_U32);
