use ruint::aliases::U256;
pub fn bitand(a: U256, b: U256) -> U256 { a.bitand(b) }
pub fn bitor(a: U256, b: U256) -> U256 { a.bitor(b) }
pub fn bitxor(a: U256, b: U256) -> U256 { a.bitxor(b) }
pub fn bitnot(a: U256) -> U256 { a.not() }
pub fn operator_and(a: U256, b: U256) -> U256 { a & b }
pub fn operator_or(a: U256, b: U256) -> U256 { a | b }
pub fn operator_xor(a: U256, b: U256) -> U256 { a ^ b }
