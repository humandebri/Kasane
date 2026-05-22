//! どこで: dropped tx ring / 何を: seq, len, evict seq計算 / なぜ: drop履歴管理を副作用から分離するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DroppedRingTransition {
    pub insert_seq: u64,
    pub next_seq: u64,
    pub len: u32,
    pub evict_seq: Option<u64>,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result.insert_seq == next_seq,
    next_seq < u64::MAX ==> result.next_seq == next_seq + 1,
    next_seq == u64::MAX ==> result.next_seq == u64::MAX,
    result.next_seq >= next_seq,
    (len as u64) < capacity ==> result.evict_seq == Option::<u64>::None,
))]
pub fn push(next_seq: u64, len: u32, capacity: u64) -> DroppedRingTransition {
    let insert_seq = next_seq;
    let next_seq = next_seq.saturating_add(1);
    if u64::from(len) < capacity {
        return DroppedRingTransition {
            insert_seq,
            next_seq,
            len: len.saturating_add(1),
            evict_seq: None,
        };
    }
    DroppedRingTransition {
        insert_seq,
        next_seq,
        len,
        evict_seq: Some(insert_seq.saturating_sub(capacity)),
    }
}

#[cfg(test)]
mod tests {
    use super::push;

    #[test]
    fn push_grows_until_capacity_then_evicts() {
        let growing = push(0, 0, 2);
        assert_eq!(growing.insert_seq, 0);
        assert_eq!(growing.next_seq, 1);
        assert_eq!(growing.len, 1);
        assert_eq!(growing.evict_seq, None);

        let full = push(2, 2, 2);
        assert_eq!(full.next_seq, 3);
        assert_eq!(full.len, 2);
        assert_eq!(full.evict_seq, Some(0));
    }
}
