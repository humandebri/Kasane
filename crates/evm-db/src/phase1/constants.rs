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

pub const RECEIPT_CONTRACT_ADDR_LEN: usize = 20;
pub const RECEIPT_CONTRACT_ADDR_LEN_U32: u32 = 20;

pub const BLOCK_BASE_SIZE_U32: u32 = 8 + HASH_LEN_U32 + HASH_LEN_U32 + 8 + HASH_LEN_U32 + HASH_LEN_U32 + 4;
pub const MAX_BLOCK_DATA_SIZE_U32: u32 = BLOCK_BASE_SIZE_U32 + (HASH_LEN_U32 * MAX_TXS_PER_BLOCK_U32);
pub const RECEIPT_SIZE_U32: u32 = 32 + 8 + 4 + 1 + 8 + 32 + 1 + RECEIPT_CONTRACT_ADDR_LEN_U32;
