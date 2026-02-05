//! どこで: wrapper運用ガード用のStableセル / 何を: cycle閾値設定と観測状態 / なぜ: 枯渇時に安全停止へ移行するため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

pub const OPS_CONFIG_SIZE_U32: u32 = 48;
pub const OPS_STATE_SIZE_U32: u32 = 48;

pub const DEFAULT_CYCLE_LOW_WATERMARK: u128 = 2_000_000_000_000;
pub const DEFAULT_CYCLE_CRITICAL: u128 = 1_000_000_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpsMode {
    Normal,
    Low,
    Critical,
}

impl OpsMode {
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Low,
            2 => Self::Critical,
            _ => Self::Normal,
        }
    }

    fn as_u8(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Low => 1,
            Self::Critical => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpsConfigV1 {
    pub low_watermark: u128,
    pub critical: u128,
    pub freeze_on_critical: bool,
}

impl OpsConfigV1 {
    pub fn new() -> Self {
        Self {
            low_watermark: DEFAULT_CYCLE_LOW_WATERMARK,
            critical: DEFAULT_CYCLE_CRITICAL,
            freeze_on_critical: true,
        }
    }
}

impl Default for OpsConfigV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Storable for OpsConfigV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; OPS_CONFIG_SIZE_U32 as usize];
        out[0..16].copy_from_slice(&self.low_watermark.to_be_bytes());
        out[16..32].copy_from_slice(&self.critical.to_be_bytes());
        out[32] = u8::from(self.freeze_on_critical);
        encode_guarded(b"ops_config", out.to_vec(), OPS_CONFIG_SIZE_U32)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != OPS_CONFIG_SIZE_U32 as usize {
            mark_decode_failure(b"ops_config", false);
            return Self::new();
        }
        let mut low = [0u8; 16];
        low.copy_from_slice(&data[0..16]);
        let mut critical = [0u8; 16];
        critical.copy_from_slice(&data[16..32]);
        let freeze_on_critical = data[32] != 0;
        Self {
            low_watermark: u128::from_be_bytes(low),
            critical: u128::from_be_bytes(critical),
            freeze_on_critical,
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: OPS_CONFIG_SIZE_U32,
        is_fixed_size: true,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpsStateV1 {
    pub last_cycle_balance: u128,
    pub last_check_ts: u64,
    pub mode: OpsMode,
    pub safe_stop_latched: bool,
}

impl OpsStateV1 {
    pub fn new() -> Self {
        Self {
            last_cycle_balance: 0,
            last_check_ts: 0,
            mode: OpsMode::Normal,
            safe_stop_latched: false,
        }
    }
}

impl Default for OpsStateV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Storable for OpsStateV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; OPS_STATE_SIZE_U32 as usize];
        out[0..16].copy_from_slice(&self.last_cycle_balance.to_be_bytes());
        out[16..24].copy_from_slice(&self.last_check_ts.to_be_bytes());
        out[24] = self.mode.as_u8();
        out[25] = u8::from(self.safe_stop_latched);
        encode_guarded(b"ops_state", out.to_vec(), OPS_STATE_SIZE_U32)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != OPS_STATE_SIZE_U32 as usize {
            mark_decode_failure(b"ops_state", false);
            return Self::new();
        }
        let mut cycle_balance = [0u8; 16];
        cycle_balance.copy_from_slice(&data[0..16]);
        let mut last_check_ts = [0u8; 8];
        last_check_ts.copy_from_slice(&data[16..24]);
        let mode = OpsMode::from_u8(data[24]);
        let safe_stop_latched = data[25] != 0;
        Self {
            last_cycle_balance: u128::from_be_bytes(cycle_balance),
            last_check_ts: u64::from_be_bytes(last_check_ts),
            mode,
            safe_stop_latched,
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: OPS_STATE_SIZE_U32,
        is_fixed_size: true,
    };
}
