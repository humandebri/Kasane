//! どこで: dropped tx の保持管理 / 何を: 固定長リング状態を保持 / なぜ: tx_locs の無限増加を防ぐため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

pub const DROPPED_RING_STATE_SIZE_U32: u32 = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DroppedRingStateV1 {
    pub schema_version: u32,
    pub next_seq: u64,
    pub len: u32,
}

impl DroppedRingStateV1 {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            next_seq: 0,
            len: 0,
        }
    }
}

impl Default for DroppedRingStateV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Storable for DroppedRingStateV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; 16];
        out[0..4].copy_from_slice(&self.schema_version.to_be_bytes());
        out[4..12].copy_from_slice(&self.next_seq.to_be_bytes());
        out[12..16].copy_from_slice(&self.len.to_be_bytes());
        encode_guarded(
            b"dropped_ring_state",
            out.to_vec(),
            DROPPED_RING_STATE_SIZE_U32,
        )
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 16 {
            mark_decode_failure(b"dropped_ring_state", false);
            return DroppedRingStateV1::new();
        }
        let mut schema = [0u8; 4];
        schema.copy_from_slice(&data[0..4]);
        let mut next_seq = [0u8; 8];
        next_seq.copy_from_slice(&data[4..12]);
        let mut len = [0u8; 4];
        len.copy_from_slice(&data[12..16]);
        Self {
            schema_version: u32::from_be_bytes(schema),
            next_seq: u64::from_be_bytes(next_seq),
            len: u32::from_be_bytes(len),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: DROPPED_RING_STATE_SIZE_U32,
        is_fixed_size: true,
    };
}
