//! どこで: BlobStoreのポインタ / 何を: BlobPtrのStorable化 / なぜ: stable上の参照を固定長で持つため

use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlobPtr {
    pub offset: u64,
    pub len: u32,
    pub class: u32,
    pub gen: u32,
}

impl Storable for BlobPtr {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; 20];
        out[0..8].copy_from_slice(&self.offset.to_be_bytes());
        out[8..12].copy_from_slice(&self.len.to_be_bytes());
        out[12..16].copy_from_slice(&self.class.to_be_bytes());
        out[16..20].copy_from_slice(&self.gen.to_be_bytes());
        Cow::Owned(out.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        let mut out = [0u8; 20];
        out[0..8].copy_from_slice(&self.offset.to_be_bytes());
        out[8..12].copy_from_slice(&self.len.to_be_bytes());
        out[12..16].copy_from_slice(&self.class.to_be_bytes());
        out[16..20].copy_from_slice(&self.gen.to_be_bytes());
        out.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 20 {
            return Self {
                offset: 0,
                len: 0,
                class: 0,
                gen: 0,
            };
        }
        let mut offset = [0u8; 8];
        offset.copy_from_slice(&data[0..8]);
        let mut len = [0u8; 4];
        len.copy_from_slice(&data[8..12]);
        let mut class = [0u8; 4];
        class.copy_from_slice(&data[12..16]);
        let mut gen = [0u8; 4];
        gen.copy_from_slice(&data[16..20]);
        Self {
            offset: u64::from_be_bytes(offset),
            len: u32::from_be_bytes(len),
            class: u32::from_be_bytes(class),
            gen: u32::from_be_bytes(gen),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 20,
        is_fixed_size: true,
    };
}
