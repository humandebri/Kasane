//! Observe the pinned standard Result::unwrap; finite samples, not an all-input proof.
use std::panic::catch_unwind;

#[test]
fn actual_result_unwrap_success_and_panic() {
    for x in [0_u64, 1, u64::MAX - 1, u64::MAX] {
        assert_eq!(Result::<u64, u8>::Ok(x).unwrap(), x);
    }
    for error in [0_u8, 13, u8::MAX] {
        let panic = catch_unwind(|| Result::<u64, u8>::Err(error).unwrap()).unwrap_err();
        let message = panic.downcast_ref::<String>().unwrap();
        assert_eq!(message, &format!("called `Result::unwrap()` on an `Err` value: {error}"));
    }
    let overflow = u64::try_from(u128::from(u64::MAX) + 1).unwrap_err();
    let panic = catch_unwind(|| Result::<u64, _>::Err(overflow).unwrap()).unwrap_err();
    let message = panic.downcast_ref::<String>().unwrap();
    assert_eq!(message, "called `Result::unwrap()` on an `Err` value: TryFromIntError(PosOverflow)");
}
