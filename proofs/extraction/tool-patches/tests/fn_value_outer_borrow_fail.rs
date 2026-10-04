fn first() -> u32 {
    1
}
fn second() -> u32 {
    2
}
fn main() {
    let mut pointer: fn() -> u32 = first;
    let outer = &mut pointer;
    pointer = second;
    let _ = outer();
}
