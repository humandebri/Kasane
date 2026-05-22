//! どこで: Phase1のQueue / 何を: submit系の順序管理 / なぜ: 決定性を保つため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;
use zerocopy::byteorder::big_endian::U64;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueMeta {
    pub head: u64,
    pub tail: u64,
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned,
)]
#[repr(C)]
struct QueueMetaWire {
    head: U64,
    tail: U64,
}

impl QueueMetaWire {
    fn new(head: u64, tail: u64) -> Self {
        Self {
            head: U64::new(head),
            tail: U64::new(tail),
        }
    }
}

impl QueueMeta {
    pub fn new() -> Self {
        Self { head: 0, tail: 0 }
    }

    pub fn is_empty(&self) -> bool {
        verified_core::queue::queue_is_empty(self.head, self.tail)
    }

    pub fn push(&mut self) -> u64 {
        let (idx, tail) = verified_core::queue::queue_push(self.tail);
        self.tail = tail;
        idx
    }

    pub fn pop(&mut self) -> Option<u64> {
        match verified_core::queue::queue_pop(self.head, self.tail) {
            Some((idx, head)) => {
                self.head = head;
                Some(idx)
            }
            None => None,
        }
    }
}

impl Default for QueueMeta {
    fn default() -> Self {
        Self::new()
    }
}

impl Storable for QueueMeta {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let wire = QueueMetaWire::new(self.head, self.tail);
        match encode_guarded(b"queue_meta", Cow::Owned(wire.as_bytes().to_vec()), 16) {
            Ok(value) => value,
            Err(_) => Cow::Owned(vec![0u8; 16]),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        let wire = QueueMetaWire::new(self.head, self.tail);
        wire.as_bytes().to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if !verified_core::stable_codec::fixed_len_matches(data.len(), 16) {
            mark_decode_failure(b"queue_meta", false);
            return QueueMeta::new();
        }
        let wire = match QueueMetaWire::read_from_bytes(data) {
            Ok(value) => value,
            Err(_) => {
                mark_decode_failure(b"queue_meta", false);
                return QueueMeta::new();
            }
        };
        Self {
            head: wire.head.get(),
            tail: wire.tail.get(),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 16,
        is_fixed_size: true,
    };
}
