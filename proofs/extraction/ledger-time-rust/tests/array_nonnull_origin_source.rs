//! Finite observations of the same transmute expression as fixed Arguments::new.
//! Borrow-instance identities must not be equated with Rust allocation addresses.
use std::ptr::NonNull;

fn retain_array_pointer<T, const N: usize>(source: &[T; N]) -> NonNull<T> {
    // The array is sized and the shared reference is non-null. No dereference here.
    unsafe { std::mem::transmute(source) }
}

#[test]
fn equal_payloads_in_distinct_nonempty_arrays_have_distinct_addresses() {
    let first = [3_u8, 5, 8];
    let second = [3_u8, 5, 8];
    assert_eq!(first, second);
    let first_pointer = retain_array_pointer(&first);
    let second_pointer = retain_array_pointer(&second);
    assert_eq!(first_pointer.as_ptr().cast_const(), first.as_ptr());
    assert_eq!(second_pointer.as_ptr().cast_const(), second.as_ptr());
    assert_ne!(first_pointer, second_pointer);
}

#[test]
fn shared_reborrows_of_one_array_keep_the_same_address() {
    let source = [13_u8, 21, 34];
    let first = &source;
    let second = &*first;
    let first_pointer = retain_array_pointer(first);
    let second_pointer = retain_array_pointer(second);
    assert_eq!(first_pointer, second_pointer);
    assert_eq!(first_pointer.as_ptr().cast_const(), source.as_ptr());
}
