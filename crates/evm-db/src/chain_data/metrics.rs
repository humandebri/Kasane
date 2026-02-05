//! どこで: Phase1.6のメトリクス / 何を: 永続カウンタとウィンドウ集計 / なぜ: 低コスト監視のため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

pub const METRICS_BUCKETS: usize = 256;
pub const METRICS_BUCKETS_U32: u32 = 256;
pub const DROP_CODE_SLOTS: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetricsBucket {
    pub block_number: u64,
    pub timestamp: u64,
    pub txs: u64,
    pub drops: u64,
}

impl MetricsBucket {
    pub fn empty() -> Self {
        Self {
            block_number: 0,
            timestamp: 0,
            txs: 0,
            drops: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetricsStateV1 {
    pub schema_version: u32,
    pub total_submitted: u64,
    pub total_included: u64,
    pub total_dropped: u64,
    pub drop_counts: [u64; DROP_CODE_SLOTS],
    pub ema_block_rate_x1000: u64,
    pub ema_txs_per_block_x1000: u64,
    pub last_ema_timestamp: u64,
    pub bucket_cursor: u32,
    pub buckets: [MetricsBucket; METRICS_BUCKETS],
}

impl MetricsStateV1 {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            total_submitted: 0,
            total_included: 0,
            total_dropped: 0,
            drop_counts: [0u64; DROP_CODE_SLOTS],
            ema_block_rate_x1000: 0,
            ema_txs_per_block_x1000: 0,
            last_ema_timestamp: 0,
            bucket_cursor: 0,
            buckets: [MetricsBucket::empty(); METRICS_BUCKETS],
        }
    }

    pub fn record_submission(&mut self, count: u64) {
        self.total_submitted = self.total_submitted.saturating_add(count);
    }

    pub fn record_drop(&mut self, code: u16, count: u64) {
        self.total_dropped = self.total_dropped.saturating_add(count);
        let idx = usize::from(code);
        if idx < DROP_CODE_SLOTS {
            self.drop_counts[idx] = self.drop_counts[idx].saturating_add(count);
        }
    }

    pub fn record_dropped(&mut self, count: u64) {
        self.total_dropped = self.total_dropped.saturating_add(count);
    }

    pub fn record_included(&mut self, count: u64) {
        self.total_included = self.total_included.saturating_add(count);
    }

    pub fn record_block(&mut self, block_number: u64, timestamp: u64, txs: u64, drops: u64) {
        self.update_ema(timestamp, txs);
        let idx = usize::try_from(self.bucket_cursor).unwrap_or(0) % METRICS_BUCKETS;
        self.buckets[idx] = MetricsBucket {
            block_number,
            timestamp,
            txs,
            drops,
        };
        self.bucket_cursor = self.bucket_cursor.wrapping_add(1) % METRICS_BUCKETS_U32;
    }

    fn update_ema(&mut self, timestamp: u64, txs: u64) {
        if timestamp == 0 {
            return;
        }
        if self.last_ema_timestamp == 0 {
            self.last_ema_timestamp = timestamp;
            self.ema_txs_per_block_x1000 = txs.saturating_mul(1000);
            return;
        }
        let delta = timestamp.saturating_sub(self.last_ema_timestamp);
        if delta == 0 {
            return;
        }
        let inst_rate_x1000 = 1000u64.saturating_div(delta);
        let inst_txs_x1000 = txs.saturating_mul(1000);
        self.ema_block_rate_x1000 =
            ((self.ema_block_rate_x1000.saturating_mul(4)).saturating_add(inst_rate_x1000)) / 5;
        self.ema_txs_per_block_x1000 =
            ((self.ema_txs_per_block_x1000.saturating_mul(4)).saturating_add(inst_txs_x1000)) / 5;
        self.last_ema_timestamp = timestamp;
    }

    pub fn window_summary(&self, window: u64) -> MetricsWindowSummary {
        let mut summary = MetricsWindowSummary::empty();
        let window = window.min(METRICS_BUCKETS as u64);
        if window == 0 {
            return summary;
        }
        let mut remaining = window as usize;
        let mut cursor = if self.bucket_cursor == 0 {
            METRICS_BUCKETS as u32
        } else {
            self.bucket_cursor
        };
        while remaining > 0 {
            cursor = cursor.wrapping_sub(1);
            let idx = usize::try_from(cursor).unwrap_or(0) % METRICS_BUCKETS;
            let bucket = self.buckets[idx];
            if bucket.timestamp == 0 && bucket.block_number == 0 {
                break;
            }
            summary.blocks = summary.blocks.saturating_add(1);
            summary.txs = summary.txs.saturating_add(bucket.txs);
            summary.drops = summary.drops.saturating_add(bucket.drops);
            if summary.first_ts == 0 {
                summary.first_ts = bucket.timestamp;
            }
            summary.last_ts = bucket.timestamp;
            remaining = remaining.saturating_sub(1);
        }
        summary
    }
}

impl Default for MetricsStateV1 {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetricsWindowSummary {
    pub blocks: u64,
    pub txs: u64,
    pub drops: u64,
    pub first_ts: u64,
    pub last_ts: u64,
}

impl MetricsWindowSummary {
    pub fn empty() -> Self {
        Self {
            blocks: 0,
            txs: 0,
            drops: 0,
            first_ts: 0,
            last_ts: 0,
        }
    }

    pub fn block_rate_per_sec_x1000(&self) -> Option<u64> {
        if self.blocks < 2 {
            return None;
        }
        let delta = self.last_ts.saturating_sub(self.first_ts);
        if delta == 0 {
            return None;
        }
        let produced = self.blocks.saturating_sub(1);
        Some(produced.saturating_mul(1000) / delta)
    }
}

impl Storable for MetricsStateV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = Vec::with_capacity(METRICS_STATE_SIZE as usize);
        out.extend_from_slice(&self.schema_version.to_be_bytes());
        out.extend_from_slice(&self.total_submitted.to_be_bytes());
        out.extend_from_slice(&self.total_included.to_be_bytes());
        out.extend_from_slice(&self.total_dropped.to_be_bytes());
        for count in self.drop_counts.iter() {
            out.extend_from_slice(&count.to_be_bytes());
        }
        out.extend_from_slice(&self.ema_block_rate_x1000.to_be_bytes());
        out.extend_from_slice(&self.ema_txs_per_block_x1000.to_be_bytes());
        out.extend_from_slice(&self.last_ema_timestamp.to_be_bytes());
        out.extend_from_slice(&self.bucket_cursor.to_be_bytes());
        out.extend_from_slice(&[0u8; 4]);
        for bucket in self.buckets.iter() {
            out.extend_from_slice(&bucket.block_number.to_be_bytes());
            out.extend_from_slice(&bucket.timestamp.to_be_bytes());
            out.extend_from_slice(&bucket.txs.to_be_bytes());
            out.extend_from_slice(&bucket.drops.to_be_bytes());
        }
        encode_guarded(b"metrics_state", out, METRICS_STATE_SIZE)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != METRICS_STATE_SIZE as usize {
            mark_decode_failure(b"metrics_state", false);
            return MetricsStateV1::new();
        }
        let mut offset = 0usize;
        let schema_version = read_u32(data, &mut offset);
        let total_submitted = read_u64(data, &mut offset);
        let total_included = read_u64(data, &mut offset);
        let total_dropped = read_u64(data, &mut offset);
        let mut drop_counts = [0u64; DROP_CODE_SLOTS];
        for slot in drop_counts.iter_mut() {
            *slot = read_u64(data, &mut offset);
        }
        let ema_block_rate_x1000 = read_u64(data, &mut offset);
        let ema_txs_per_block_x1000 = read_u64(data, &mut offset);
        let last_ema_timestamp = read_u64(data, &mut offset);
        let bucket_cursor = read_u32(data, &mut offset);
        offset = offset.saturating_add(4);
        let mut buckets = [MetricsBucket::empty(); METRICS_BUCKETS];
        for bucket in buckets.iter_mut() {
            bucket.block_number = read_u64(data, &mut offset);
            bucket.timestamp = read_u64(data, &mut offset);
            bucket.txs = read_u64(data, &mut offset);
            bucket.drops = read_u64(data, &mut offset);
        }
        Self {
            schema_version,
            total_submitted,
            total_included,
            total_dropped,
            drop_counts,
            ema_block_rate_x1000,
            ema_txs_per_block_x1000,
            last_ema_timestamp,
            bucket_cursor,
            buckets,
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: METRICS_STATE_SIZE_U32,
        is_fixed_size: true,
    };
}

const METRICS_BUCKET_SIZE: u32 = 8 * 4;
const METRICS_STATE_BASE: u32 = 4 + 8 * 3 + 8 * (DROP_CODE_SLOTS as u32) + 8 * 3 + 4 + 4;
const METRICS_STATE_SIZE_U32: u32 = METRICS_STATE_BASE + METRICS_BUCKET_SIZE * METRICS_BUCKETS_U32;
const METRICS_STATE_SIZE: u32 = METRICS_STATE_SIZE_U32;

fn read_u64(data: &[u8], offset: &mut usize) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&data[*offset..*offset + 8]);
    *offset += 8;
    u64::from_be_bytes(buf)
}

fn read_u32(data: &[u8], offset: &mut usize) -> u32 {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&data[*offset..*offset + 4]);
    *offset += 4;
    u32::from_be_bytes(buf)
}

#[cfg(test)]
mod tests {
    use super::MetricsStateV1;

    #[test]
    fn ema_updates_with_alpha_point_two() {
        let mut metrics = MetricsStateV1::new();
        metrics.record_block(1, 10, 5, 0);
        assert_eq!(metrics.ema_txs_per_block_x1000, 5000);
        metrics.record_block(2, 15, 1, 0);
        assert_eq!(metrics.ema_block_rate_x1000, 40);
        assert_eq!(metrics.ema_txs_per_block_x1000, 4200);
    }
}
