use std::time::Duration;

#[test]
fn source_duration_fields_and_wide_arithmetic_boundaries() {
    for n in [
        0,
        1,
        999_999_999,
        1_000_000_000,
        1_000_000_001,
        u64::MAX - 1,
        u64::MAX,
    ] {
        let d = Duration::from_nanos(n);
        assert_eq!(d.as_secs(), n / 1_000_000_000);
        assert_eq!(u64::from(d.subsec_nanos()), n % 1_000_000_000);
        assert_eq!(d.as_nanos(), u128::from(n));
    }
    for secs in [0, 1, u64::MAX / 1_000_000_000, u64::MAX - 1, u64::MAX] {
        for nanos in [0, 1, 999_999_999] {
            let d = Duration::new(secs, nanos);
            assert_eq!(
                d.as_nanos(),
                u128::from(secs) * 1_000_000_000 + u128::from(nanos)
            );
        }
    }
}
