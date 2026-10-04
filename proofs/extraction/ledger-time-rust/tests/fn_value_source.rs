type WriteFn = for<'a, 'b> fn(&'a u32, &'b mut u32);

fn write(input: &u32, output: &mut u32) {
    *output = *input;
}

fn duplicate<T: Copy>(value: T) -> (T, T) {
    (value, value)
}

#[test]
fn copied_pointer_can_be_reused_after_call_arguments_expire() {
    let (first, second) = duplicate(write as WriteFn);
    for input in [0, 17, u32::MAX] {
        let mut output = 0;
        first(&input, &mut output);
        assert_eq!(output, input);
        output = 1;
        second(&input, &mut output);
        assert_eq!(output, input);
    }
}

fn identity(input: &u32) -> &u32 {
    input
}

#[test]
fn reference_return_keeps_the_input_identity() {
    let (first, second) = duplicate(identity as for<'a> fn(&'a u32) -> &'a u32);
    let input = 23;
    assert!(std::ptr::eq(first(&input), &input));
    assert!(std::ptr::eq(second(&input), &input));
}

#[test]
fn mutable_outer_reference_updates_the_pointer() {
    fn first() -> u32 {
        1
    }
    fn second() -> u32 {
        2
    }
    let mut pointer: fn() -> u32 = first;
    {
        let outer = &mut pointer;
        assert_eq!(outer(), 1);
        *outer = second;
        assert_eq!(outer(), 2);
    }
    assert_eq!(pointer(), 2);
}
