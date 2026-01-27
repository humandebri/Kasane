//! どこで: StableBTreeMapのValue / 何を: 固定長Value定義 / なぜ: 互換性と決定性を守るため

pub const ACCOUNT_NONCE_LEN: usize = 8;
pub const ACCOUNT_BALANCE_LEN: usize = 32;
pub const ACCOUNT_CODE_HASH_LEN: usize = 32;
pub const ACCOUNT_VAL_LEN: usize = ACCOUNT_NONCE_LEN + ACCOUNT_BALANCE_LEN + ACCOUNT_CODE_HASH_LEN;
pub const ACCOUNT_VAL_LEN_U32: u32 = 72;

pub const U256_LEN: usize = 32;
pub const U256_LEN_U32: u32 = 32;

pub const MAX_CODE_SIZE: usize = 24 * 1024;
pub const MAX_CODE_SIZE_U32: u32 = 24 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccountVal(pub [u8; ACCOUNT_VAL_LEN]);

impl AccountVal {
    pub fn from_parts(nonce: u64, balance: [u8; ACCOUNT_BALANCE_LEN], code_hash: [u8; ACCOUNT_CODE_HASH_LEN]) -> Self {
        let mut buf = [0u8; ACCOUNT_VAL_LEN];
        buf[0..ACCOUNT_NONCE_LEN].copy_from_slice(&nonce.to_be_bytes());
        buf[ACCOUNT_NONCE_LEN..ACCOUNT_NONCE_LEN + ACCOUNT_BALANCE_LEN].copy_from_slice(&balance);
        buf[ACCOUNT_NONCE_LEN + ACCOUNT_BALANCE_LEN..ACCOUNT_VAL_LEN].copy_from_slice(&code_hash);
        Self(buf)
    }

    pub fn nonce(&self) -> u64 {
        let mut bytes = [0u8; ACCOUNT_NONCE_LEN];
        bytes.copy_from_slice(&self.0[0..ACCOUNT_NONCE_LEN]);
        u64::from_be_bytes(bytes)
    }

    pub fn balance(&self) -> [u8; ACCOUNT_BALANCE_LEN] {
        let mut out = [0u8; ACCOUNT_BALANCE_LEN];
        out.copy_from_slice(&self.0[ACCOUNT_NONCE_LEN..ACCOUNT_NONCE_LEN + ACCOUNT_BALANCE_LEN]);
        out
    }

    pub fn code_hash(&self) -> [u8; ACCOUNT_CODE_HASH_LEN] {
        let mut out = [0u8; ACCOUNT_CODE_HASH_LEN];
        out.copy_from_slice(&self.0[ACCOUNT_NONCE_LEN + ACCOUNT_BALANCE_LEN..ACCOUNT_VAL_LEN]);
        out
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct U256Val(pub [u8; U256_LEN]);

impl U256Val {
    pub fn new(bytes: [u8; U256_LEN]) -> Self {
        Self(bytes)
    }

    pub fn bytes(&self) -> [u8; U256_LEN] {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeVal(pub Vec<u8>);
