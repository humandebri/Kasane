#![feature(try_from_int_error_kind)]
use core::num::{IntErrorKind, TryFromIntError};

#[test]
fn actual_u128_to_u64_error_payload() {
    assert_eq!(core::mem::size_of::<TryFromIntError>(), core::mem::size_of::<IntErrorKind>());
    for input in [0, 1, u128::from(u64::MAX) - 1, u128::from(u64::MAX),
                  u128::from(u64::MAX) + 1, u128::from(u64::MAX) + 2, u128::MAX] {
        match u64::try_from(input) {
            Ok(value) => {
                assert!(input <= u128::from(u64::MAX));
                assert_eq!(u128::from(value), input);
            }
            Err(error) => {
                assert!(input > u128::from(u64::MAX));
                assert_eq!(error.kind(), &IntErrorKind::PosOverflow);
            }
        }
    }
}
