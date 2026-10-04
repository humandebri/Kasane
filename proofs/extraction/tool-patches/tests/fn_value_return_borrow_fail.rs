fn identity(input: &u32) -> &u32 {
    input
}
fn main() {
    let pointer: for<'a> fn(&'a u32) -> &'a u32 = identity;
    let returned;
    {
        let input = 42;
        returned = pointer(&input);
    }
    let _ = *returned;
}
