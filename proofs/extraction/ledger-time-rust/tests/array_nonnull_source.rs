use std::mem::{align_of, size_of, transmute};
use std::ptr::{self, NonNull};

// The fixed Arguments::new body uses this representation cast for each array.
// The pointer may be dereferenced only while its shared source is alive and N>0.
unsafe fn retain_array_pointer<T, const N: usize>(source: &[T; N]) -> NonNull<T> {
    unsafe { transmute(source) }
}

fn check_pointer<T, const N: usize>(source: &[T; N]) -> NonNull<T> {
    assert_eq!(size_of::<&[T; N]>(), size_of::<NonNull<T>>());
    assert_eq!(align_of::<&[T; N]>(), align_of::<NonNull<T>>());
    let pointer = unsafe { retain_array_pointer(source) };
    assert!(ptr::addr_eq(pointer.as_ptr(), source.as_ptr()));
    pointer
}

#[test]
fn byte_array_preserves_first_element_and_shared_source() {
    let source = [4_u8, 9, 13, 21];
    let pointer = check_pointer(&source);
    assert_eq!(unsafe { *pointer.as_ref() }, source[0]);
    assert_eq!(source, [4, 9, 13, 21]);
}

#[test]
fn empty_array_pointer_is_retained_without_dereference() {
    let source: [u8; 0] = [];
    let _ = check_pointer(&source);
}

#[test]
fn zero_sized_element_pointer_remains_valid_for_nonempty_array() {
    #[derive(Debug, PartialEq)]
    struct Empty;
    let source = [Empty, Empty, Empty];
    let pointer = check_pointer(&source);
    assert_eq!(unsafe { pointer.as_ref() }, &source[0]);
    let empty: [Empty; 0] = [];
    let _ = check_pointer(&empty);
}

#[test]
fn aligned_element_pointer_preserves_alignment_and_payload() {
    #[repr(align(64))]
    struct Aligned([u8; 3]);
    let source = [Aligned([1, 2, 3]), Aligned([5, 8, 13])];
    let pointer = check_pointer(&source);
    assert_eq!(pointer.as_ptr().addr() % align_of::<Aligned>(), 0);
    assert_eq!(unsafe { pointer.as_ref() }.0, [1, 2, 3]);
}

#[test]
fn nested_shared_reference_payload_retains_identity() {
    let value = 42_u64;
    let source = [&value, &value];
    let pointer = check_pointer(&source);
    assert!(ptr::eq(unsafe { *pointer.as_ref() }, &value));
}
