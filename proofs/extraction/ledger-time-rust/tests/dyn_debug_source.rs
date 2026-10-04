//! Finite observations of the pinned Rust dynamic Debug dispatch.
use std::cell::Cell;
use std::fmt;

struct Probe<'a> {
    value: i32,
    calls: &'a Cell<u32>,
    alternate: &'a Cell<bool>,
}
impl fmt::Debug for Probe<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.calls.set(self.calls.get() + 1);
        self.alternate.set(f.alternate());
        write!(f, "probe({})", self.value)
    }
}
#[test]
fn actual_dyn_debug_uses_receiver_and_impl() {
    for value in [i32::MIN, -1, 0, 1, i32::MAX] {
        let calls = Cell::new(0);
        let alternate = Cell::new(false);
        let probe = Probe {
            value,
            calls: &calls,
            alternate: &alternate,
        };
        let object: &dyn fmt::Debug = &probe;
        assert_eq!(format!("{object:?}"), format!("probe({value})"));
        assert_eq!(calls.get(), 1);
        assert!(!alternate.get());
        assert_eq!(format!("{object:#?}"), format!("probe({value})"));
        assert_eq!(calls.get(), 2);
        assert!(alternate.get());
    }
}
