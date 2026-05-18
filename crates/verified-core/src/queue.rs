//! どこで: pending queue上限 / 何を: sender/principal/global cap判定 / なぜ: 副作用前のevict判断を検証可能にするため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingCapDecision {
    Accept,
    SenderFull,
    PrincipalFull,
    GlobalFull,
    EvictLowest,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PendingCapInput {
    pub sender_count: usize,
    pub principal_count: usize,
    pub global_count: u64,
    pub max_per_sender: usize,
    pub max_per_principal: usize,
    pub max_global: u64,
    pub incoming_effective_gas_price: u64,
    pub lowest_effective_gas_price: Option<u64>,
}

#[cfg_attr(verus_keep_ghost, verus_spec(empty => ensures
    empty == (head == tail),
))]
pub fn queue_is_empty(head: u64, tail: u64) -> bool {
    head == tail
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result.0 == tail,
    tail < u64::MAX ==> result.1 == tail + 1,
    tail == u64::MAX ==> result.1 == u64::MAX,
    result.1 >= tail,
))]
pub fn queue_push(tail: u64) -> (u64, u64) {
    (tail, tail.saturating_add(1))
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    head == tail ==> result == Option::<(u64, u64)>::None,
    head != tail ==> matches!(result, Some(_)),
))]
pub fn queue_pop(head: u64, tail: u64) -> Option<(u64, u64)> {
    if queue_is_empty(head, tail) {
        None
    } else {
        Some((head, head.saturating_add(1)))
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(decision => ensures
    input.sender_count >= input.max_per_sender ==> decision == PendingCapDecision::SenderFull,
    input.sender_count < input.max_per_sender && input.principal_count >= input.max_per_principal
        ==> decision == PendingCapDecision::PrincipalFull,
    input.sender_count < input.max_per_sender
        && input.principal_count < input.max_per_principal
        && input.global_count < input.max_global
        ==> decision == PendingCapDecision::Accept,
    input.sender_count < input.max_per_sender
        && input.principal_count < input.max_per_principal
        && input.global_count >= input.max_global
        && matches!(input.lowest_effective_gas_price, Some(_))
        && input.incoming_effective_gas_price > input.lowest_effective_gas_price.unwrap()
        ==> decision == PendingCapDecision::EvictLowest,
))]
pub fn classify_pending_caps(input: PendingCapInput) -> PendingCapDecision {
    if input.sender_count >= input.max_per_sender {
        return PendingCapDecision::SenderFull;
    }
    if input.principal_count >= input.max_per_principal {
        return PendingCapDecision::PrincipalFull;
    }
    if input.global_count < input.max_global {
        return PendingCapDecision::Accept;
    }
    match input.lowest_effective_gas_price {
        Some(lowest) if input.incoming_effective_gas_price > lowest => {
            PendingCapDecision::EvictLowest
        }
        _ => PendingCapDecision::GlobalFull,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_pending_caps, queue_is_empty, queue_pop, queue_push, PendingCapDecision,
        PendingCapInput,
    };

    fn base() -> PendingCapInput {
        PendingCapInput {
            sender_count: 0,
            principal_count: 0,
            global_count: 0,
            max_per_sender: 2,
            max_per_principal: 3,
            max_global: 4,
            incoming_effective_gas_price: 10,
            lowest_effective_gas_price: Some(5),
        }
    }

    #[test]
    fn classify_pending_caps_orders_limits() {
        assert_eq!(classify_pending_caps(base()), PendingCapDecision::Accept);
        assert_eq!(
            classify_pending_caps(PendingCapInput {
                sender_count: 2,
                ..base()
            }),
            PendingCapDecision::SenderFull
        );
        assert_eq!(
            classify_pending_caps(PendingCapInput {
                principal_count: 3,
                ..base()
            }),
            PendingCapDecision::PrincipalFull
        );
    }

    #[test]
    fn classify_pending_caps_eviction_requires_higher_fee() {
        assert_eq!(
            classify_pending_caps(PendingCapInput {
                global_count: 4,
                ..base()
            }),
            PendingCapDecision::EvictLowest
        );
        assert_eq!(
            classify_pending_caps(PendingCapInput {
                global_count: 4,
                incoming_effective_gas_price: 5,
                ..base()
            }),
            PendingCapDecision::GlobalFull
        );
    }

    #[test]
    fn queue_cursor_transitions_are_monotonic() {
        assert!(queue_is_empty(0, 0));
        let (seq, tail) = queue_push(0);
        assert_eq!((seq, tail), (0, 1));
        assert_eq!(queue_pop(0, 1), Some((0, 1)));
        assert_eq!(queue_pop(1, 1), None);
    }
}
