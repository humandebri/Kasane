//! どこで: evm-core共通ユーティリティ / 何を: 現在時刻(sec)取得を集約 / なぜ: cfg分岐を呼び出し側から隔離するため

#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
static TEST_NOW_SEC: AtomicU64 = AtomicU64::new(0);

#[allow(dead_code)]
pub(crate) fn now_sec() -> u64 {
    #[cfg(test)]
    {
        let injected = TEST_NOW_SEC.load(Ordering::Relaxed);
        if injected != 0 {
            return injected;
        }
    }

    now_ns() / 1_000_000_000
}

#[cfg(test)]
pub(crate) fn set_test_now_sec(value: u64) {
    TEST_NOW_SEC.store(value, Ordering::Relaxed);
}

#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
fn now_ns() -> u64 {
    ic_cdk::api::time()
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
fn now_ns() -> u64 {
    let nanos_u128 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);

    let max_u64 = u128::from(u64::MAX);
    let clamped = nanos_u128.min(max_u64);
    match u64::try_from(clamped) {
        Ok(value) => value,
        Err(_) => u64::MAX,
    }
}
