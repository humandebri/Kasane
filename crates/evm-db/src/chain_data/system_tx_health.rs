//! どこで: system tx運用観測セル / 何を: backoff状態と失敗統計を保持 / なぜ: 実行副作用と運用観測を分離するため

use crate::corrupt_log::record_corrupt;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

pub const SYSTEM_TX_HEALTH_SIZE_U32: u32 = 48;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SystemTxHealthV1 {
    pub schema_version: u8,
    pub consecutive_failures: u64,
    pub last_fail_ts: u64,
    pub last_warn_ts: u64,
    pub backoff_until_ts: u64,
    pub backoff_hits: u64,
}

impl SystemTxHealthV1 {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            consecutive_failures: 0,
            last_fail_ts: 0,
            last_warn_ts: 0,
            backoff_until_ts: 0,
            backoff_hits: 0,
        }
    }
}

impl Default for SystemTxHealthV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Storable for SystemTxHealthV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; SYSTEM_TX_HEALTH_SIZE_U32 as usize];
        out[0] = self.schema_version;
        out[8..16].copy_from_slice(&self.consecutive_failures.to_be_bytes());
        out[16..24].copy_from_slice(&self.last_fail_ts.to_be_bytes());
        out[24..32].copy_from_slice(&self.last_warn_ts.to_be_bytes());
        out[32..40].copy_from_slice(&self.backoff_until_ts.to_be_bytes());
        out[40..48].copy_from_slice(&self.backoff_hits.to_be_bytes());
        Cow::Owned(out.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != SYSTEM_TX_HEALTH_SIZE_U32 as usize {
            record_corrupt(b"system_tx_health");
            return Self::new();
        }
        let mut consecutive_failures = [0u8; 8];
        consecutive_failures.copy_from_slice(&data[8..16]);
        let mut last_fail_ts = [0u8; 8];
        last_fail_ts.copy_from_slice(&data[16..24]);
        let mut last_warn_ts = [0u8; 8];
        last_warn_ts.copy_from_slice(&data[24..32]);
        let mut backoff_until_ts = [0u8; 8];
        backoff_until_ts.copy_from_slice(&data[32..40]);
        let mut backoff_hits = [0u8; 8];
        backoff_hits.copy_from_slice(&data[40..48]);
        Self {
            schema_version: data[0],
            consecutive_failures: u64::from_be_bytes(consecutive_failures),
            last_fail_ts: u64::from_be_bytes(last_fail_ts),
            last_warn_ts: u64::from_be_bytes(last_warn_ts),
            backoff_until_ts: u64::from_be_bytes(backoff_until_ts),
            backoff_hits: u64::from_be_bytes(backoff_hits),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: SYSTEM_TX_HEALTH_SIZE_U32,
        is_fixed_size: true,
    };
}
