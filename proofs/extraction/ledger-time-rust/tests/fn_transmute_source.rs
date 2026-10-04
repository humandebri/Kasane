use std::fmt::{self, Debug, Display, Formatter};
use std::marker::PhantomData;
use std::ptr::NonNull;

#[derive(Clone, Copy)]
struct Argument<'a> {
    value: NonNull<()>,
    formatter: unsafe fn(NonNull<()>, &mut Formatter<'_>) -> fmt::Result,
    lifetime: PhantomData<&'a ()>,
}

impl<'a> Argument<'a> {
    fn debug<T: Debug>(value: &'a T) -> Self {
        let formatter: fn(&T, &mut Formatter<'_>) -> fmt::Result = T::fmt;
        Self {
            value: NonNull::from_ref(value).cast(),
            // The private fields pair this callback with the same live T.
            formatter: unsafe { std::mem::transmute(formatter) },
            lifetime: PhantomData,
        }
    }

    fn display<T: Display>(value: &'a T) -> Self {
        let formatter: fn(&T, &mut Formatter<'_>) -> fmt::Result = T::fmt;
        Self {
            value: NonNull::from_ref(value).cast(),
            // The private fields pair this callback with the same live T.
            formatter: unsafe { std::mem::transmute(formatter) },
            lifetime: PhantomData,
        }
    }
}

impl Debug for Argument<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        // The constructors maintain the live receiver and callback pairing.
        unsafe { (self.formatter)(self.value, formatter) }
    }
}

impl Display for Argument<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        unsafe { (self.formatter)(self.value, formatter) }
    }
}

#[test]
fn debug_scalars_and_copies_preserve_observations() {
    for value in [0_u64, 17, u64::MAX] {
        let first = Argument::debug(&value);
        let second = first;
        assert_eq!(format!("{first:?}"), format!("{value:?}"));
        assert_eq!(format!("{second:#?}"), format!("{value:#?}"));
        assert_eq!(format!("{first:>24?}"), format!("{value:>24?}"));
    }
}

#[test]
fn display_receivers_with_internal_slice_metadata_work() {
    for value in ["", "λ", "a\nb"] {
        let argument = Argument::display(&value);
        assert_eq!(format!("{argument}"), format!("{value}"));
        assert_eq!(format!("{argument:>8.2}"), format!("{value:>8.2}"));
    }
}

#[test]
fn receiver_and_formatter_flags_reach_the_callback() {
    use std::cell::Cell;
    struct Spy {
        calls: Cell<u32>,
        flags: Cell<(bool, Option<usize>, Option<usize>)>,
    }
    impl Debug for Spy {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            self.calls.set(self.calls.get() + 1);
            self.flags.set((f.alternate(), f.width(), f.precision()));
            f.write_str("seen")
        }
    }
    let receiver = Spy {
        calls: Cell::new(0),
        flags: Cell::new((false, None, None)),
    };
    let argument = Argument::debug(&receiver);
    assert_eq!(format!("{argument:#7.3?}"), "seen");
    assert_eq!(receiver.calls.get(), 1);
    assert_eq!(receiver.flags.get(), (true, Some(7), Some(3)));
}

#[test]
fn callback_error_is_preserved() {
    struct Failing;
    impl Debug for Failing {
        fn fmt(&self, _: &mut Formatter<'_>) -> fmt::Result {
            Err(fmt::Error)
        }
    }
    let receiver = Failing;
    let argument = Argument::debug(&receiver);
    let mut output = String::new();
    assert!(fmt::write(&mut output, format_args!("{argument:?}")).is_err());
}

#[test]
fn zero_sized_receiver_remains_valid() {
    let receiver = ();
    let argument = Argument::debug(&receiver);
    assert_eq!(format!("{argument:?}"), format!("{receiver:?}"));
}
