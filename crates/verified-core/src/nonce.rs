//! どこで: submit時nonce判定 / 何を: 期待nonceと置換可否 / なぜ: queue変更前に純粋な決定を固定するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[allow(dead_code)]
fn main() {}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NonceDecision {
    Accept,
    TooLow,
    Gap,
    Conflict,
    Replace,
}

pub fn classify_nonce_raw(
    expected_nonce: u64,
    incoming_nonce: u64,
    pending_effective_gas_price_present: u64,
    pending_effective_gas_price_value: u64,
    incoming_effective_gas_price: u64,
) -> NonceDecision {
    if incoming_nonce < expected_nonce {
        return NonceDecision::TooLow;
    }
    if incoming_nonce > expected_nonce {
        return NonceDecision::Gap;
    }
    if pending_effective_gas_price_present != 1 {
        return NonceDecision::Accept;
    }
    if incoming_effective_gas_price <= pending_effective_gas_price_value {
        NonceDecision::Conflict
    } else {
        NonceDecision::Replace
    }
}

pub fn classify_nonce(
    expected_nonce: u64,
    incoming_nonce: u64,
    pending_effective_gas_price: Option<u64>,
    incoming_effective_gas_price: u64,
) -> NonceDecision {
    match pending_effective_gas_price {
        Some(old_effective) => classify_nonce_raw(
            expected_nonce,
            incoming_nonce,
            1,
            old_effective,
            incoming_effective_gas_price,
        ),
        None => classify_nonce_raw(
            expected_nonce,
            incoming_nonce,
            0,
            0,
            incoming_effective_gas_price,
        ),
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(next => ensures
    current < u64::MAX ==> next == current + 1,
    current == u64::MAX ==> next == u64::MAX,
    next >= current,
))]
pub fn bump_expected_nonce(current: u64) -> u64 {
    current.saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::{bump_expected_nonce, classify_nonce, classify_nonce_raw, NonceDecision};

    #[test]
    fn classify_nonce_rejects_low_and_gap() {
        assert_eq!(classify_nonce(10, 9, None, 1), NonceDecision::TooLow);
        assert_eq!(classify_nonce(10, 11, None, 1), NonceDecision::Gap);
    }

    #[test]
    fn classify_nonce_handles_current_and_replacement() {
        assert_eq!(classify_nonce(10, 10, None, 1), NonceDecision::Accept);
        assert_eq!(
            classify_nonce(10, 10, Some(100), 100),
            NonceDecision::Conflict
        );
        assert_eq!(
            classify_nonce(10, 10, Some(100), 101),
            NonceDecision::Replace
        );
    }

    #[test]
    fn classify_nonce_adapter_matches_raw_model() {
        assert_eq!(
            classify_nonce(10, 10, None, 100),
            classify_nonce_raw(10, 10, 0, 0, 100)
        );
        assert_eq!(
            classify_nonce(10, 10, Some(100), 101),
            classify_nonce_raw(10, 10, 1, 100, 101)
        );
    }

    #[test]
    fn bump_expected_nonce_saturates() {
        assert_eq!(bump_expected_nonce(0), 1);
        assert_eq!(bump_expected_nonce(u64::MAX), u64::MAX);
    }
}
