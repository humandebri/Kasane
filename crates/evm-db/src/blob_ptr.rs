//! どこで: BlobStoreのポインタ / 何を: BlobPtrのStorable化 / なぜ: stable上の参照を固定長で持つため

use crate::corrupt_log::record_corrupt;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlobPtr([u8; 20]);

impl BlobPtr {
    pub fn new(offset: u64, len: u32, class: u32, gen: u32) -> Self {
        let mut out = [0u8; 20];
        out[0..8].copy_from_slice(&offset.to_be_bytes());
        out[8..12].copy_from_slice(&len.to_be_bytes());
        out[12..16].copy_from_slice(&class.to_be_bytes());
        out[16..20].copy_from_slice(&gen.to_be_bytes());
        Self(out)
    }

    pub fn offset(&self) -> u64 {
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&self.0[0..8]);
        u64::from_be_bytes(buf)
    }

    pub fn len(&self) -> u32 {
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&self.0[8..12]);
        u32::from_be_bytes(buf)
    }

    pub fn class(&self) -> u32 {
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&self.0[12..16]);
        u32::from_be_bytes(buf)
    }

    pub fn gen(&self) -> u32 {
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&self.0[16..20]);
        u32::from_be_bytes(buf)
    }
}

impl Storable for BlobPtr {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(&self.0)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 20 {
            record_corrupt(b"blob_ptr");
            return Self {
                0: [0u8; 20],
            };
        }
        let mut out = [0u8; 20];
        out.copy_from_slice(data);
        Self(out)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 20,
        is_fixed_size: true,
    };
}
