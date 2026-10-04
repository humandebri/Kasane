use ruint::aliases::U256;
pub fn add(a: U256, b: U256) -> U256 { a.wrapping_add(b) }
pub fn sub(a: U256, b: U256) -> U256 { a.wrapping_sub(b) }
