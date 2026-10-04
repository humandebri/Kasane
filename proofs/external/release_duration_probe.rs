//! Actual release-compiler standard-library calls; this is an extraction probe.
use std::num::TryFromIntError;
use std::time::Duration;

pub fn duration_nanos(duration: Duration) -> u128 {
    duration.as_nanos()
}

pub fn checked_duration_nanos(duration: Duration) -> Result<u64, TryFromIntError> {
    duration.as_nanos().try_into()
}

pub fn add_duration(nanos: u64, duration: Duration) -> u64 {
    nanos.saturating_add(duration.as_nanos().try_into().unwrap())
}

pub fn sub_duration(nanos: u64, duration: Duration) -> u64 {
    nanos.saturating_sub(duration.as_nanos().try_into().unwrap())
}
