#![feature(core_intrinsics)]
#![allow(internal_features)]

pub fn ub_checks() -> bool {
    core::intrinsics::ub_checks()
}
