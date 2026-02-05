//! どこで: chain_data 共通codec補助 / 何を: Bound防波堤とdecode方針補助 / なぜ: 破損時の扱いを統一するため

use crate::corrupt_log::record_corrupt;
use std::borrow::Cow;

pub fn encode_guarded<'a>(label: &'static [u8], bytes: Vec<u8>, max_size: u32) -> Cow<'a, [u8]> {
    ensure_encoded_within_bound(label, bytes.len(), max_size);
    Cow::Owned(bytes)
}

pub fn ensure_encoded_within_bound(label: &'static [u8], encoded_len: usize, max_size: u32) {
    if encoded_len > max_size as usize {
        record_corrupt(label);
        ic_cdk::trap("storable.encode.bound_exceeded");
    }
}

pub fn mark_decode_failure(label: &'static [u8], fail_closed: bool) {
    record_corrupt(label);
    if fail_closed {
        crate::meta::set_needs_migration(true);
    }
}
