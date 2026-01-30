//! どこで: pruningの状態管理 / 何を: pruned_before_block等の保持 / なぜ: None判定を安定させるため

use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

const PRUNE_STATE_SIZE_U32: u32 = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PruneStateV1 {
    pub schema_version: u32,
    pub pruned_before_block: u64,
    pub next_prune_block: u64,
}

impl PruneStateV1 {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            pruned_before_block: u64::MAX,
            next_prune_block: 0,
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
}

impl Default for PruneStateV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Storable for PruneStateV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; 24];
        out[0..4].copy_from_slice(&self.schema_version.to_be_bytes());
        out[4..12].copy_from_slice(&self.pruned_before_block.to_be_bytes());
        out[12..20].copy_from_slice(&self.next_prune_block.to_be_bytes());
        Cow::Owned(out.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 24 {
            ic_cdk::trap("prune_state: invalid length");
        }
        let mut schema = [0u8; 4];
        schema.copy_from_slice(&data[0..4]);
        let mut pruned_before_block = [0u8; 8];
        pruned_before_block.copy_from_slice(&data[4..12]);
        let mut next_prune_block = [0u8; 8];
        next_prune_block.copy_from_slice(&data[12..20]);
        Self {
            schema_version: u32::from_be_bytes(schema),
            pruned_before_block: u64::from_be_bytes(pruned_before_block),
            next_prune_block: u64::from_be_bytes(next_prune_block),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: PRUNE_STATE_SIZE_U32,
        is_fixed_size: true,
    };
}
