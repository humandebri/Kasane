#[test]
fn empty_array_dereference_is_invalid() {
    let source: [u8; 0] = [];
    let pointer: std::ptr::NonNull<u8> = unsafe { std::mem::transmute(&source) };
    // Miri-only negative: the retained pointer does not establish an element.
    let _ = unsafe { pointer.as_ref() };
}
