fn main() {
    let captured = 42_u32;
    let pointer: fn() -> u32 = || captured;
    let _ = pointer();
}
