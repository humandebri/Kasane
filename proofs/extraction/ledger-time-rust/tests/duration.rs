use kasane_ledger_timestamp_probe::{add_time, from_nanos, get_nanos, sub_time};
use std::{panic::catch_unwind, time::Duration};

#[test]
fn nanosecond_wrappers_follow_saturating_boundaries() {
    let values = [
        0,
        1,
        999_999_999,
        1_000_000_000,
        1_000_000_001,
        u64::MAX / 2,
        u64::MAX - 1,
        u64::MAX,
    ];
    for n in values {
        for d in values {
            let sum = (u128::from(n) + u128::from(d)).min(u128::from(u64::MAX));
            assert_eq!(get_nanos(add_time(n, d)), u64::try_from(sum).unwrap());
            assert_eq!(get_nanos(sub_time(n, d)), n.saturating_sub(d));
        }
    }
}

#[test]
fn duration_conversion_boundary_controls_both_trait_operations() {
    let billion = 1_000_000_000;
    let last_secs = u64::MAX / billion;
    let last_nanos = u32::try_from(u64::MAX % billion).unwrap();
    let durations = [
        Duration::ZERO,
        Duration::from_nanos(u64::MAX),
        Duration::new(last_secs, last_nanos + 1),
        Duration::from_secs(last_secs + 1),
        Duration::new(u64::MAX, 999_999_999),
    ];
    for n in [0, 1, u64::MAX] {
        for d in durations {
            let add = catch_unwind(|| get_nanos(from_nanos(n) + d));
            let sub = catch_unwind(|| get_nanos(from_nanos(n) - d));
            match u64::try_from(d.as_nanos()) {
                Ok(v) => {
                    assert_eq!(add.unwrap(), n.saturating_add(v));
                    assert_eq!(sub.unwrap(), n.saturating_sub(v));
                }
                Err(_) => {
                    assert!(add.is_err());
                    assert!(sub.is_err());
                }
            }
        }
    }
}
