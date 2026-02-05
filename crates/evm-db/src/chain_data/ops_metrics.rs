//! どこで: wrapper運用観測の補助セル / 何を: 実行警告メトリクスを保持 / なぜ: OpsStateの固定サイズを壊さないため

use crate::corrupt_log::record_corrupt;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

pub const OPS_METRICS_SIZE_U32: u32 = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpsMetricsV1 {
    pub schema_version: u8,
    pub exec_halt_unknown_count: u64,
    pub last_exec_halt_unknown_warn_ts: u64,
}

impl OpsMetricsV1 {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            exec_halt_unknown_count: 0,
            last_exec_halt_unknown_warn_ts: 0,
        }
    }
}

impl Default for OpsMetricsV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Storable for OpsMetricsV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; OPS_METRICS_SIZE_U32 as usize];
        out[0] = self.schema_version;
        out[8..16].copy_from_slice(&self.exec_halt_unknown_count.to_be_bytes());
        out[16..24].copy_from_slice(&self.last_exec_halt_unknown_warn_ts.to_be_bytes());
        Cow::Owned(out.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != OPS_METRICS_SIZE_U32 as usize && data.len() != 40 {
            record_corrupt(b"ops_metrics");
            return Self::new();
        }
        let mut unknown_count = [0u8; 8];
        unknown_count.copy_from_slice(&data[8..16]);
        let mut last_warn = [0u8; 8];
        last_warn.copy_from_slice(&data[16..24]);
        Self {
            schema_version: data[0],
            exec_halt_unknown_count: u64::from_be_bytes(unknown_count),
            last_exec_halt_unknown_warn_ts: u64::from_be_bytes(last_warn),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: OPS_METRICS_SIZE_U32,
        is_fixed_size: true,
    };
}
