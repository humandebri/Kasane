//! どこで: Phase1のQueue / 何を: submit系の順序管理 / なぜ: 決定性を保つため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueMeta {
    pub head: u64,
    pub tail: u64,
}

impl QueueMeta {
    pub fn new() -> Self {
        Self { head: 0, tail: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    pub fn push(&mut self) -> u64 {
        let idx = self.tail;
        self.tail = self.tail.saturating_add(1);
        idx
    }

    pub fn pop(&mut self) -> Option<u64> {
        if self.is_empty() {
            None
        } else {
            let idx = self.head;
            self.head = self.head.saturating_add(1);
            Some(idx)
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
        let mut out = [0u8; 16];
        out[0..8].copy_from_slice(&self.head.to_be_bytes());
        out[8..16].copy_from_slice(&self.tail.to_be_bytes());
        encode_guarded(b"queue_meta", out.to_vec(), 16)
    }

    fn into_bytes(self) -> Vec<u8> {
        let mut out = [0u8; 16];
        out[0..8].copy_from_slice(&self.head.to_be_bytes());
        out[8..16].copy_from_slice(&self.tail.to_be_bytes());
        out.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 16 {
            mark_decode_failure(b"queue_meta", false);
            return QueueMeta::new();
        }
        let mut head = [0u8; 8];
        head.copy_from_slice(&data[0..8]);
        let mut tail = [0u8; 8];
        tail.copy_from_slice(&data[8..16]);
        Self {
            head: u64::from_be_bytes(head),
            tail: u64::from_be_bytes(tail),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 16,
        is_fixed_size: true,
    };
}
