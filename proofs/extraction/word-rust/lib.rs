pub fn carrying_add(lhs: u64, rhs: u64, carry: bool) -> (u64, bool) {
    ruint::algorithms::carrying_add(lhs, rhs, carry)
}

pub fn borrowing_sub(lhs: u64, rhs: u64, borrow: bool) -> (u64, bool) {
    ruint::algorithms::borrowing_sub(lhs, rhs, borrow)
}
