//! どこで: pruningの状態管理 / 何を: pruned_before_block等の保持 / なぜ: None判定を安定させるため

use crate::blob_ptr::BlobPtr;
use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use crate::corrupt_log::record_corrupt;
use crate::chain_data::constants::MAX_TXS_PER_BLOCK_U32;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

const PRUNE_STATE_SIZE_U32: u32 = 32;
const JOURNAL_NONE: u64 = u64::MAX;
const MAX_PTRS_U32: u32 = 1 + (2 * MAX_TXS_PER_BLOCK_U32);
const JOURNAL_MAX_SIZE_U32: u32 = 4 + (MAX_PTRS_U32 * 20);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PruneStateV1 {
    pub schema_version: u32,
    pub pruned_before_block: u64,
    pub next_prune_block: u64,
    pub journal_block_number: u64,
}

impl PruneStateV1 {
    pub fn new() -> Self {
        Self {
            schema_version: 2,
            pruned_before_block: u64::MAX,
            next_prune_block: 0,
            journal_block_number: JOURNAL_NONE,
        }
    }

    pub fn pruned_before(&self) -> Option<u64> {
        if self.pruned_before_block == u64::MAX {
            None
        } else {
            Some(self.pruned_before_block)
        }
    }

    pub fn set_pruned_before(&mut self, value: u64) {
        self.pruned_before_block = value;
    }

    pub fn journal_block(&self) -> Option<u64> {
        if self.journal_block_number == JOURNAL_NONE {
            None
        } else {
            Some(self.journal_block_number)
        }
    }

    pub fn set_journal_block(&mut self, value: u64) {
        self.journal_block_number = value;
    }

    pub fn clear_journal(&mut self) {
        self.journal_block_number = JOURNAL_NONE;
    }
}

impl Default for PruneStateV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Storable for PruneStateV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; 32];
        out[0..4].copy_from_slice(&self.schema_version.to_be_bytes());
        out[4..12].copy_from_slice(&self.pruned_before_block.to_be_bytes());
        out[12..20].copy_from_slice(&self.next_prune_block.to_be_bytes());
        out[20..28].copy_from_slice(&self.journal_block_number.to_be_bytes());
        match encode_guarded(b"prune_state", out.to_vec(), PRUNE_STATE_SIZE_U32) {
            Ok(value) => value,
            Err(_) => Cow::Owned(vec![0u8; PRUNE_STATE_SIZE_U32 as usize]),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 32 {
            mark_decode_failure(b"prune_state", false);
            return PruneStateV1::new();
        }
        let mut schema = [0u8; 4];
        schema.copy_from_slice(&data[0..4]);
        let mut pruned_before_block = [0u8; 8];
        pruned_before_block.copy_from_slice(&data[4..12]);
        let mut next_prune_block = [0u8; 8];
        next_prune_block.copy_from_slice(&data[12..20]);
        let mut journal_block_number = [0u8; 8];
        journal_block_number.copy_from_slice(&data[20..28]);
        Self {
            schema_version: u32::from_be_bytes(schema),
            pruned_before_block: u64::from_be_bytes(pruned_before_block),
            next_prune_block: u64::from_be_bytes(next_prune_block),
            journal_block_number: u64::from_be_bytes(journal_block_number),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: PRUNE_STATE_SIZE_U32,
        is_fixed_size: true,
    };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PruneJournal {
    pub ptrs: Vec<BlobPtr>,
}

impl Storable for PruneJournal {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let len = match u32::try_from(self.ptrs.len()) {
            Ok(value) => value,
            Err(_) => {
                record_corrupt(b"prune_journal_len");
                return encode_fallback_prune_journal();
            }
        };
        if len > MAX_PTRS_U32 {
            record_corrupt(b"prune_journal_len");
            return encode_fallback_prune_journal();
        }
        let mut out = Vec::with_capacity(4 + (self.ptrs.len() * 20));
        out.extend_from_slice(&len.to_be_bytes());
        for ptr in self.ptrs.iter() {
            let bytes = ptr.to_bytes();
            out.extend_from_slice(&bytes);
        }
        match encode_guarded(b"prune_journal", out, JOURNAL_MAX_SIZE_U32) {
            Ok(value) => value,
            Err(_) => Cow::Owned(vec![0u8; JOURNAL_MAX_SIZE_U32 as usize]),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() < 4 {
            mark_decode_failure(b"prune_journal", false);
            return PruneJournal { ptrs: Vec::new() };
        }
        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&data[0..4]);
        let len = u32::from_be_bytes(len_bytes);
        if len > MAX_PTRS_U32 {
            mark_decode_failure(b"prune_journal", false);
            return PruneJournal { ptrs: Vec::new() };
        }
        let expected = 4usize
            .checked_add((len as usize).saturating_mul(20))
            .unwrap_or(0);
        if data.len() != expected {
            mark_decode_failure(b"prune_journal", false);
            return PruneJournal { ptrs: Vec::new() };
        }
        let mut ptrs = Vec::with_capacity(len as usize);
        let mut offset = 4usize;
        for _ in 0..len {
            let end = offset + 20;
            if end > data.len() {
                mark_decode_failure(b"prune_journal", false);
                return PruneJournal { ptrs: Vec::new() };
            }
            let ptr = BlobPtr::from_bytes(Cow::Owned(data[offset..end].to_vec()));
            ptrs.push(ptr);
            offset = end;
        }
        Self { ptrs }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: JOURNAL_MAX_SIZE_U32,
        is_fixed_size: false,
    };
}

fn encode_fallback_prune_journal() -> Cow<'static, [u8]> {
    let mut out = Vec::with_capacity(4);
    out.extend_from_slice(&0u32.to_be_bytes());
    match encode_guarded(b"prune_journal", out, JOURNAL_MAX_SIZE_U32) {
        Ok(value) => value,
        Err(_) => Cow::Owned(vec![0u8; JOURNAL_MAX_SIZE_U32 as usize]),
    }
}
