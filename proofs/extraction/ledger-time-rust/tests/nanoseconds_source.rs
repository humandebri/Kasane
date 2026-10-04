#![feature(temporary_niche_types)]
use core::num::niche_types::Nanoseconds;

#[test]
fn actual_nanoseconds_layout_and_valid_casts() {
    assert_eq!(core::mem::size_of::<Nanoseconds>(), core::mem::size_of::<u32>());
    assert_eq!(core::mem::align_of::<Nanoseconds>(), core::mem::align_of::<u32>());
    for value in [0, 1, 2, 999_999_998, 999_999_999] {
        let checked = Nanoseconds::new(value).unwrap();
        assert_eq!(checked.as_inner(), value);
        // SAFETY: each selected value satisfies the actual type's documented range.
        let unchecked = unsafe { Nanoseconds::new_unchecked(value) };
        assert_eq!(unchecked.as_inner(), value);
    }
    for value in [1_000_000_000, u32::MAX] {
        assert!(Nanoseconds::new(value).is_none());
    }
}
