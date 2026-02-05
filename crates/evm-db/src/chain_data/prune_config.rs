//! どこで: pruning設定とメトリクス / 何を: policy+状態の固定サイズ / なぜ: upgrade耐性と安全運用のため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

const PRUNE_CONFIG_SIZE_U32: u32 = 112;
const NONE_U64: u64 = u64::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrunePolicy {
    pub target_bytes: u64,
    pub retain_days: u64,
    pub retain_blocks: u64,
    pub headroom_ratio_bps: u32,
    pub hard_emergency_ratio_bps: u32,
    pub timer_interval_ms: u64,
    pub max_ops_per_tick: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PruneConfigV1 {
    pub schema_version: u32,
    pub pruning_enabled: bool,
    pub prune_running: bool,
    pub prune_scheduled: bool,
    pub target_bytes: u64,
    pub retain_days: u64,
    pub retain_blocks: u64,
    pub headroom_ratio_bps: u32,
    pub hard_emergency_ratio_bps: u32,
    pub timer_interval_ms: u64,
    pub max_ops_per_tick: u32,
    pub high_water_bytes: u64,
    pub low_water_bytes: u64,
    pub hard_emergency_bytes: u64,
    pub estimated_kept_bytes: u64,
    pub oldest_kept_block: u64,
    pub oldest_kept_timestamp: u64,
    pub last_prune_at: u64,
}

impl PruneConfigV1 {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            pruning_enabled: false,
            prune_running: false,
            prune_scheduled: false,
            target_bytes: 0,
            retain_days: 0,
            retain_blocks: 0,
            headroom_ratio_bps: 2000,
            hard_emergency_ratio_bps: 9500,
            timer_interval_ms: 60_000,
            max_ops_per_tick: 5_000,
            high_water_bytes: 0,
            low_water_bytes: 0,
            hard_emergency_bytes: 0,
            estimated_kept_bytes: 0,
            oldest_kept_block: NONE_U64,
            oldest_kept_timestamp: NONE_U64,
            last_prune_at: 0,
        }
    }

    pub fn policy(&self) -> PrunePolicy {
        PrunePolicy {
            target_bytes: self.target_bytes,
            retain_days: self.retain_days,
            retain_blocks: self.retain_blocks,
            headroom_ratio_bps: self.headroom_ratio_bps,
            hard_emergency_ratio_bps: self.hard_emergency_ratio_bps,
            timer_interval_ms: self.timer_interval_ms,
            max_ops_per_tick: self.max_ops_per_tick,
        }
    }

    pub fn set_policy(&mut self, policy: PrunePolicy) {
        self.target_bytes = policy.target_bytes;
        self.retain_days = policy.retain_days;
        self.retain_blocks = policy.retain_blocks;
        self.headroom_ratio_bps = policy.headroom_ratio_bps;
        self.hard_emergency_ratio_bps = policy.hard_emergency_ratio_bps;
        self.timer_interval_ms = policy.timer_interval_ms;
        self.max_ops_per_tick = policy.max_ops_per_tick;
        self.high_water_bytes = compute_high_water(policy.target_bytes, policy.headroom_ratio_bps);
        self.low_water_bytes = compute_low_water(policy.target_bytes, policy.headroom_ratio_bps);
        self.hard_emergency_bytes =
            compute_ratio_bytes(policy.target_bytes, policy.hard_emergency_ratio_bps);
    }

    pub fn oldest_block(&self) -> Option<u64> {
        if self.oldest_kept_block == NONE_U64 {
            None
        } else {
            Some(self.oldest_kept_block)
        }
    }

    pub fn oldest_timestamp(&self) -> Option<u64> {
        if self.oldest_kept_timestamp == NONE_U64 {
            None
        } else {
            Some(self.oldest_kept_timestamp)
        }
    }

    pub fn set_oldest(&mut self, block: u64, timestamp: u64) {
        self.oldest_kept_block = block;
        self.oldest_kept_timestamp = timestamp;
    }

    pub fn clear_oldest(&mut self) {
        self.oldest_kept_block = NONE_U64;
        self.oldest_kept_timestamp = NONE_U64;
    }
}

impl Default for PruneConfigV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Storable for PruneConfigV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; PRUNE_CONFIG_SIZE_U32 as usize];
        out[0..4].copy_from_slice(&self.schema_version.to_be_bytes());
        out[4] = flags(
            self.pruning_enabled,
            self.prune_running,
            self.prune_scheduled,
        );
        out[8..16].copy_from_slice(&self.target_bytes.to_be_bytes());
        out[16..24].copy_from_slice(&self.retain_days.to_be_bytes());
        out[24..32].copy_from_slice(&self.retain_blocks.to_be_bytes());
        out[32..36].copy_from_slice(&self.headroom_ratio_bps.to_be_bytes());
        out[36..40].copy_from_slice(&self.hard_emergency_ratio_bps.to_be_bytes());
        out[40..48].copy_from_slice(&self.timer_interval_ms.to_be_bytes());
        out[48..52].copy_from_slice(&self.max_ops_per_tick.to_be_bytes());
        out[56..64].copy_from_slice(&self.high_water_bytes.to_be_bytes());
        out[64..72].copy_from_slice(&self.low_water_bytes.to_be_bytes());
        out[72..80].copy_from_slice(&self.hard_emergency_bytes.to_be_bytes());
        out[80..88].copy_from_slice(&self.estimated_kept_bytes.to_be_bytes());
        out[88..96].copy_from_slice(&self.oldest_kept_block.to_be_bytes());
        out[96..104].copy_from_slice(&self.oldest_kept_timestamp.to_be_bytes());
        out[104..112].copy_from_slice(&self.last_prune_at.to_be_bytes());
        encode_guarded(b"prune_config", out.to_vec(), PRUNE_CONFIG_SIZE_U32)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != PRUNE_CONFIG_SIZE_U32 as usize {
            mark_decode_failure(b"prune_config", false);
            return PruneConfigV1::new();
        }
        let mut schema = [0u8; 4];
        schema.copy_from_slice(&data[0..4]);
        let flags = data[4];
        let mut target_bytes = [0u8; 8];
        target_bytes.copy_from_slice(&data[8..16]);
        let mut retain_days = [0u8; 8];
        retain_days.copy_from_slice(&data[16..24]);
        let mut retain_blocks = [0u8; 8];
        retain_blocks.copy_from_slice(&data[24..32]);
        let mut headroom_ratio_bps = [0u8; 4];
        headroom_ratio_bps.copy_from_slice(&data[32..36]);
        let mut hard_emergency_ratio_bps = [0u8; 4];
        hard_emergency_ratio_bps.copy_from_slice(&data[36..40]);
        let mut timer_interval_ms = [0u8; 8];
        timer_interval_ms.copy_from_slice(&data[40..48]);
        let mut max_ops_per_tick = [0u8; 4];
        max_ops_per_tick.copy_from_slice(&data[48..52]);
        let mut high_water_bytes = [0u8; 8];
        high_water_bytes.copy_from_slice(&data[56..64]);
        let mut low_water_bytes = [0u8; 8];
        low_water_bytes.copy_from_slice(&data[64..72]);
        let mut hard_emergency_bytes = [0u8; 8];
        hard_emergency_bytes.copy_from_slice(&data[72..80]);
        let mut estimated_kept_bytes = [0u8; 8];
        estimated_kept_bytes.copy_from_slice(&data[80..88]);
        let mut oldest_kept_block = [0u8; 8];
        oldest_kept_block.copy_from_slice(&data[88..96]);
        let mut oldest_kept_timestamp = [0u8; 8];
        oldest_kept_timestamp.copy_from_slice(&data[96..104]);
        let mut last_prune_at = [0u8; 8];
        last_prune_at.copy_from_slice(&data[104..112]);
        let (pruning_enabled, prune_running, prune_scheduled) = decode_flags(flags);
        Self {
            schema_version: u32::from_be_bytes(schema),
            pruning_enabled,
            prune_running,
            prune_scheduled,
            target_bytes: u64::from_be_bytes(target_bytes),
            retain_days: u64::from_be_bytes(retain_days),
            retain_blocks: u64::from_be_bytes(retain_blocks),
            headroom_ratio_bps: u32::from_be_bytes(headroom_ratio_bps),
            hard_emergency_ratio_bps: u32::from_be_bytes(hard_emergency_ratio_bps),
            timer_interval_ms: u64::from_be_bytes(timer_interval_ms),
            max_ops_per_tick: u32::from_be_bytes(max_ops_per_tick),
            high_water_bytes: u64::from_be_bytes(high_water_bytes),
            low_water_bytes: u64::from_be_bytes(low_water_bytes),
            hard_emergency_bytes: u64::from_be_bytes(hard_emergency_bytes),
            estimated_kept_bytes: u64::from_be_bytes(estimated_kept_bytes),
            oldest_kept_block: u64::from_be_bytes(oldest_kept_block),
            oldest_kept_timestamp: u64::from_be_bytes(oldest_kept_timestamp),
            last_prune_at: u64::from_be_bytes(last_prune_at),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: PRUNE_CONFIG_SIZE_U32,
        is_fixed_size: true,
    };
}

fn flags(pruning_enabled: bool, prune_running: bool, prune_scheduled: bool) -> u8 {
    let mut out = 0u8;
    if pruning_enabled {
        out |= 1 << 0;
    }
    if prune_running {
        out |= 1 << 1;
    }
    if prune_scheduled {
        out |= 1 << 2;
    }
    out
}

fn decode_flags(value: u8) -> (bool, bool, bool) {
    (
        (value & (1 << 0)) != 0,
        (value & (1 << 1)) != 0,
        (value & (1 << 2)) != 0,
    )
}

fn compute_ratio_bytes(target: u64, ratio_bps: u32) -> u64 {
    let numerator = u128::from(target).saturating_mul(u128::from(ratio_bps));
    let value = numerator / 10_000u128;
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn compute_high_water(target: u64, headroom_bps: u32) -> u64 {
    if headroom_bps == 0 {
        return target;
    }
    let half = headroom_bps / 2;
    let ratio = 10_000u32.saturating_sub(half);
    compute_ratio_bytes(target, ratio)
}

fn compute_low_water(target: u64, headroom_bps: u32) -> u64 {
    if headroom_bps == 0 {
        return target;
    }
    let ratio = 10_000u32.saturating_sub(headroom_bps);
    compute_ratio_bytes(target, ratio)
}

#[cfg(test)]
mod tests {
    use super::{
        compute_high_water, compute_low_water, compute_ratio_bytes, PruneConfigV1, PrunePolicy,
    };

    #[test]
    fn ratio_bytes_rounds_down() {
        assert_eq!(compute_ratio_bytes(1000, 5000), 500);
    }

    #[test]
    fn headroom_bounds() {
        let target = 1_000_000u64;
        let headroom = 2000u32;
        let high = compute_high_water(target, headroom);
        let low = compute_low_water(target, headroom);
        assert!(high > low);
        assert!(high <= target);
    }

    #[test]
    fn set_policy_updates_watermarks() {
        let mut config = PruneConfigV1::new();
        let policy = PrunePolicy {
            target_bytes: 1_000_000,
            retain_days: 7,
            retain_blocks: 100,
            headroom_ratio_bps: 2000,
            hard_emergency_ratio_bps: 9500,
            timer_interval_ms: 30_000,
            max_ops_per_tick: 1000,
        };
        config.set_policy(policy);
        assert!(config.high_water_bytes > 0);
        assert!(config.low_water_bytes > 0);
        assert!(config.hard_emergency_bytes > 0);
    }
}
