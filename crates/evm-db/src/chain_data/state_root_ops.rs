//! どこで: state root運用情報 / 何を: 検証メトリクス・不一致記録・移行状態 / なぜ: fail-closed運用を再現可能にするため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

pub const STATE_ROOT_METRICS_SIZE_U32: u32 = 72;
pub const STATE_ROOT_MISMATCH_SIZE_U32: u32 = 152;
pub const STATE_ROOT_MIGRATION_SIZE_U32: u32 = 32;
pub const STATE_ROOT_NODE_RECORD_MAX_U32: u32 = 2048;
pub const STATE_ROOT_GC_STATE_SIZE_U32: u32 = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct HashKey(pub [u8; 32]);

impl Storable for HashKey {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        encode_guarded(b"state_root_hash_key", self.0.to_vec(), 32)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 32 {
            mark_decode_failure(b"state_root_hash_key", false);
            return HashKey([0u8; 32]);
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(data);
        Self(out)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 32,
        is_fixed_size: true,
    };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeRecord {
    pub refcnt: u32,
    pub rlp: Vec<u8>,
}

impl NodeRecord {
    pub fn new(refcnt: u32, rlp: Vec<u8>) -> Self {
        Self { refcnt, rlp }
    }
}

impl Storable for NodeRecord {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let len = u32::try_from(self.rlp.len()).unwrap_or(0);
        let mut out = Vec::with_capacity(8 + self.rlp.len());
        out.extend_from_slice(&self.refcnt.to_be_bytes());
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&self.rlp);
        encode_guarded(
            b"state_root_node_record",
            out,
            STATE_ROOT_NODE_RECORD_MAX_U32,
        )
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() < 8 {
            mark_decode_failure(b"state_root_node_record", false);
            return NodeRecord::new(0, Vec::new());
        }
        let mut refcnt = [0u8; 4];
        refcnt.copy_from_slice(&data[0..4]);
        let mut len = [0u8; 4];
        len.copy_from_slice(&data[4..8]);
        let rlp_len = usize::try_from(u32::from_be_bytes(len)).unwrap_or(0);
        if data.len() != 8 + rlp_len {
            mark_decode_failure(b"state_root_node_record", false);
            return NodeRecord::new(0, Vec::new());
        }
        NodeRecord::new(u32::from_be_bytes(refcnt), data[8..].to_vec())
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: STATE_ROOT_NODE_RECORD_MAX_U32,
        is_fixed_size: false,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GcStateV1 {
    pub enqueue_seq: u64,
    pub dequeue_seq: u64,
    pub len: u64,
}

impl GcStateV1 {
    pub fn new() -> Self {
        Self {
            enqueue_seq: 0,
            dequeue_seq: 0,
            len: 0,
        }
    }
}

impl Storable for GcStateV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; 32];
        out[0..8].copy_from_slice(&self.enqueue_seq.to_be_bytes());
        out[8..16].copy_from_slice(&self.dequeue_seq.to_be_bytes());
        out[16..24].copy_from_slice(&self.len.to_be_bytes());
        encode_guarded(
            b"state_root_gc_state",
            out.to_vec(),
            STATE_ROOT_GC_STATE_SIZE_U32,
        )
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 32 {
            mark_decode_failure(b"state_root_gc_state", false);
            return Self::new();
        }
        let mut enqueue = [0u8; 8];
        enqueue.copy_from_slice(&data[0..8]);
        let mut dequeue = [0u8; 8];
        dequeue.copy_from_slice(&data[8..16]);
        let mut len = [0u8; 8];
        len.copy_from_slice(&data[16..24]);
        Self {
            enqueue_seq: u64::from_be_bytes(enqueue),
            dequeue_seq: u64::from_be_bytes(dequeue),
            len: u64::from_be_bytes(len),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: STATE_ROOT_GC_STATE_SIZE_U32,
        is_fixed_size: true,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MigrationPhase {
    Init = 0,
    BuildTrie = 1,
    BuildRefcnt = 2,
    Verify = 3,
    Done = 4,
}

impl MigrationPhase {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Init,
            1 => Self::BuildTrie,
            2 => Self::BuildRefcnt,
            3 => Self::Verify,
            4 => Self::Done,
            _ => Self::Init,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MigrationStateV1 {
    pub schema_version: u32,
    pub phase: MigrationPhase,
    pub cursor: u64,
    pub last_error: u32,
    pub schema_version_target: u32,
}

impl MigrationStateV1 {
    pub fn new_done(schema_version_target: u32) -> Self {
        Self {
            schema_version: 1,
            phase: MigrationPhase::Done,
            cursor: 0,
            last_error: 0,
            schema_version_target,
        }
    }
}

impl Storable for MigrationStateV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; 32];
        out[0..4].copy_from_slice(&self.schema_version.to_be_bytes());
        out[4] = self.phase as u8;
        out[8..16].copy_from_slice(&self.cursor.to_be_bytes());
        out[16..20].copy_from_slice(&self.last_error.to_be_bytes());
        out[20..24].copy_from_slice(&self.schema_version_target.to_be_bytes());
        encode_guarded(
            b"state_root_migration",
            out.to_vec(),
            STATE_ROOT_MIGRATION_SIZE_U32,
        )
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 32 {
            mark_decode_failure(b"state_root_migration", false);
            return MigrationStateV1::new_done(1);
        }
        let mut schema = [0u8; 4];
        schema.copy_from_slice(&data[0..4]);
        let mut cursor = [0u8; 8];
        cursor.copy_from_slice(&data[8..16]);
        let mut last_error = [0u8; 4];
        last_error.copy_from_slice(&data[16..20]);
        let mut target = [0u8; 4];
        target.copy_from_slice(&data[20..24]);
        Self {
            schema_version: u32::from_be_bytes(schema),
            phase: MigrationPhase::from_u8(data[4]),
            cursor: u64::from_be_bytes(cursor),
            last_error: u32::from_be_bytes(last_error),
            schema_version_target: u32::from_be_bytes(target),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: STATE_ROOT_MIGRATION_SIZE_U32,
        is_fixed_size: true,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StateRootMetricsV1 {
    pub schema_version: u32,
    pub state_root_mismatch_count: u64,
    pub state_root_verify_count: u64,
    pub state_root_verify_skipped_count: u64,
    pub node_db_entries: u64,
    pub node_db_reachable: u64,
    pub node_db_unreachable: u64,
    pub gc_progress: u64,
    pub migration_phase: u8,
}

impl StateRootMetricsV1 {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            state_root_mismatch_count: 0,
            state_root_verify_count: 0,
            state_root_verify_skipped_count: 0,
            node_db_entries: 0,
            node_db_reachable: 0,
            node_db_unreachable: 0,
            gc_progress: 0,
            migration_phase: MigrationPhase::Done as u8,
        }
    }
}

impl Storable for StateRootMetricsV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; 72];
        out[0..4].copy_from_slice(&self.schema_version.to_be_bytes());
        out[8..16].copy_from_slice(&self.state_root_mismatch_count.to_be_bytes());
        out[16..24].copy_from_slice(&self.state_root_verify_count.to_be_bytes());
        out[24..32].copy_from_slice(&self.state_root_verify_skipped_count.to_be_bytes());
        out[32..40].copy_from_slice(&self.node_db_entries.to_be_bytes());
        out[40..48].copy_from_slice(&self.node_db_reachable.to_be_bytes());
        out[48..56].copy_from_slice(&self.node_db_unreachable.to_be_bytes());
        out[56..64].copy_from_slice(&self.gc_progress.to_be_bytes());
        out[64] = self.migration_phase;
        encode_guarded(
            b"state_root_metrics",
            out.to_vec(),
            STATE_ROOT_METRICS_SIZE_U32,
        )
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 72 {
            mark_decode_failure(b"state_root_metrics", false);
            return StateRootMetricsV1::new();
        }
        let mut schema = [0u8; 4];
        schema.copy_from_slice(&data[0..4]);
        let mut mismatch = [0u8; 8];
        mismatch.copy_from_slice(&data[8..16]);
        let mut verify = [0u8; 8];
        verify.copy_from_slice(&data[16..24]);
        let mut skipped = [0u8; 8];
        skipped.copy_from_slice(&data[24..32]);
        let mut entries = [0u8; 8];
        entries.copy_from_slice(&data[32..40]);
        let mut reachable = [0u8; 8];
        reachable.copy_from_slice(&data[40..48]);
        let mut unreachable = [0u8; 8];
        unreachable.copy_from_slice(&data[48..56]);
        let mut gc = [0u8; 8];
        gc.copy_from_slice(&data[56..64]);
        Self {
            schema_version: u32::from_be_bytes(schema),
            state_root_mismatch_count: u64::from_be_bytes(mismatch),
            state_root_verify_count: u64::from_be_bytes(verify),
            state_root_verify_skipped_count: u64::from_be_bytes(skipped),
            node_db_entries: u64::from_be_bytes(entries),
            node_db_reachable: u64::from_be_bytes(reachable),
            node_db_unreachable: u64::from_be_bytes(unreachable),
            gc_progress: u64::from_be_bytes(gc),
            migration_phase: data[64],
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: STATE_ROOT_METRICS_SIZE_U32,
        is_fixed_size: true,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MismatchRecordV1 {
    pub block_number: u64,
    pub parent_hash: [u8; 32],
    pub incremental_root: [u8; 32],
    pub reference_root: [u8; 32],
    pub touched_accounts_count: u32,
    pub touched_slots_count: u32,
    pub delta_digest: [u8; 32],
    pub timestamp: u64,
}

impl Storable for MismatchRecordV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; 152];
        out[0..8].copy_from_slice(&self.block_number.to_be_bytes());
        out[8..40].copy_from_slice(&self.parent_hash);
        out[40..72].copy_from_slice(&self.incremental_root);
        out[72..104].copy_from_slice(&self.reference_root);
        out[104..108].copy_from_slice(&self.touched_accounts_count.to_be_bytes());
        out[108..112].copy_from_slice(&self.touched_slots_count.to_be_bytes());
        out[112..144].copy_from_slice(&self.delta_digest);
        out[144..152].copy_from_slice(&self.timestamp.to_be_bytes());
        encode_guarded(
            b"state_root_mismatch_record",
            out.to_vec(),
            STATE_ROOT_MISMATCH_SIZE_U32,
        )
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 152 {
            mark_decode_failure(b"state_root_mismatch_record", false);
            return Self {
                block_number: 0,
                parent_hash: [0u8; 32],
                incremental_root: [0u8; 32],
                reference_root: [0u8; 32],
                touched_accounts_count: 0,
                touched_slots_count: 0,
                delta_digest: [0u8; 32],
                timestamp: 0,
            };
        }
        let mut block = [0u8; 8];
        block.copy_from_slice(&data[0..8]);
        let mut parent_hash = [0u8; 32];
        parent_hash.copy_from_slice(&data[8..40]);
        let mut incremental_root = [0u8; 32];
        incremental_root.copy_from_slice(&data[40..72]);
        let mut reference_root = [0u8; 32];
        reference_root.copy_from_slice(&data[72..104]);
        let mut touched_accounts = [0u8; 4];
        touched_accounts.copy_from_slice(&data[104..108]);
        let mut touched_slots = [0u8; 4];
        touched_slots.copy_from_slice(&data[108..112]);
        let mut delta_digest = [0u8; 32];
        delta_digest.copy_from_slice(&data[112..144]);
        let mut timestamp = [0u8; 8];
        timestamp.copy_from_slice(&data[144..152]);
        Self {
            block_number: u64::from_be_bytes(block),
            parent_hash,
            incremental_root,
            reference_root,
            touched_accounts_count: u32::from_be_bytes(touched_accounts),
            touched_slots_count: u32::from_be_bytes(touched_slots),
            delta_digest,
            timestamp: u64::from_be_bytes(timestamp),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: STATE_ROOT_MISMATCH_SIZE_U32,
        is_fixed_size: true,
    };
}

#[cfg(test)]
mod tests {
    use super::{NodeRecord, Storable};
    use std::borrow::Cow;

    #[test]
    fn node_record_roundtrip_large_rlp() {
        let record = NodeRecord::new(3, vec![0xabu8; 600]);
        let bytes = record.to_bytes().into_owned();
        let decoded = NodeRecord::from_bytes(Cow::Owned(bytes));
        assert_eq!(decoded.refcnt, 3);
        assert_eq!(decoded.rlp.len(), 600);
        assert!(decoded.rlp.iter().all(|b| *b == 0xab));
    }
}
