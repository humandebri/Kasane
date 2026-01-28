//! どこで: META領域 / 何を: magic/version/schema_hashの検証 / なぜ: 壊れた起動を防ぐため

use crate::memory::{get_memory, AppMemoryId, VMem};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::{StableCell, Storable};
use std::borrow::Cow;

const META_MAGIC: [u8; 4] = *b"EVM0";
const META_LAYOUT_VERSION: u32 = 1;
#[allow(dead_code)]
const META_SCHEMA_STRING: &str = "mem:0..4|keys:v1|ic_tx:rlp-fixed|merkle:v1|env:v1";
// Keccak-256(META_SCHEMA_STRING)
const META_SCHEMA_HASH: [u8; 32] = [
    0x6d, 0x56, 0xd9, 0x15, 0xe8, 0x9e, 0x5d, 0x97,
    0xd7, 0xbe, 0xc4, 0xe0, 0x52, 0xa0, 0xc7, 0xb1,
    0xc1, 0xd3, 0x38, 0x12, 0x71, 0x77, 0x5e, 0xdd,
    0x07, 0xc2, 0xc5, 0xa4, 0x77, 0xd5, 0xa8, 0x1b,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Meta {
    pub magic: [u8; 4],
    pub layout_version: u32,
    pub schema_hash: [u8; 32],
}

impl Meta {
    pub fn new() -> Self {
        Self {
            magic: META_MAGIC,
            layout_version: META_LAYOUT_VERSION,
            schema_hash: META_SCHEMA_HASH,
        }
    }
}

impl Default for Meta {
    fn default() -> Self {
        Self::new()
    }
}

impl Storable for Meta {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut buf = [0u8; 40];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..8].copy_from_slice(&self.layout_version.to_be_bytes());
        buf[8..40].copy_from_slice(&self.schema_hash);
        Cow::Owned(buf.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        let mut buf = [0u8; 40];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..8].copy_from_slice(&self.layout_version.to_be_bytes());
        buf[8..40].copy_from_slice(&self.schema_hash);
        buf.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 40 {
            ic_cdk::trap("meta: invalid length");
        }
        let mut magic = [0u8; 4];
        let mut schema_hash = [0u8; 32];
        magic.copy_from_slice(&data[0..4]);
        let layout_version = u32::from_be_bytes([
            data[4], data[5], data[6], data[7],
        ]);
        schema_hash.copy_from_slice(&data[8..40]);
        Self {
            magic,
            layout_version,
            schema_hash,
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 40,
        is_fixed_size: true,
    };
}

fn init_meta_cell() -> StableCell<Meta, VMem> {
    StableCell::init(get_memory(AppMemoryId::Meta), Meta::new())
}

pub fn init_meta_or_trap() {
    let cell = init_meta_cell();
    let meta = cell.get();
    if meta.magic != META_MAGIC {
        ic_cdk::trap("meta: magic mismatch");
    }
    if meta.layout_version != META_LAYOUT_VERSION {
        ic_cdk::trap("meta: layout_version mismatch");
    }
    if meta.schema_hash != META_SCHEMA_HASH {
        ic_cdk::trap("meta: schema_hash mismatch");
    }
}
