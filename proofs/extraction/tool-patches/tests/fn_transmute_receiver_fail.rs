use std::fmt::{self, Formatter};
use std::ptr::NonNull;
struct WrongPair {
    value: NonNull<()>,
    formatter: unsafe fn(NonNull<()>, &mut Formatter<'_>) -> fmt::Result,
}
impl fmt::Debug for WrongPair {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        unsafe { (self.formatter)(self.value, f) }
    }
}
#[test]
fn wrong_receiver_must_be_rejected_by_miri() {
    let receiver = 42_u8;
    let typed: fn(&u64, &mut Formatter<'_>) -> fmt::Result = fmt::Debug::fmt;
    let wrong = WrongPair {
        value: NonNull::from_ref(&receiver).cast(),
        formatter: unsafe { std::mem::transmute(typed) },
    };
    let _ = format!("{wrong:?}");
}
