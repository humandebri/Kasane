mod timestamp;
use timestamp::TimeStamp;
use std::time::Duration;
pub fn add_time(nanos: u64, duration: u64) -> TimeStamp {
    TimeStamp::from_nanos_since_unix_epoch(nanos) + Duration::from_nanos(duration)
}
pub fn sub_time(nanos: u64, duration: u64) -> TimeStamp {
    TimeStamp::from_nanos_since_unix_epoch(nanos) - Duration::from_nanos(duration)
}

pub fn from_nanos(nanos: u64) -> TimeStamp { TimeStamp::from_nanos_since_unix_epoch(nanos) }
pub fn get_nanos(t: TimeStamp) -> u64 { t.as_nanos_since_unix_epoch() }
pub fn new_time(secs: u64, nanos: u32) -> TimeStamp { TimeStamp::new(secs, nanos) }
