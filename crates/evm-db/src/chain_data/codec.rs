//! どこで: chain_data 共通codec補助 / 何を: Bound防波堤とdecode方針補助 / なぜ: 破損時の扱いを統一するため

use crate::corrupt_log::record_corrupt;
use std::borrow::Cow;

pub fn encode_guarded<'a>(label: &'static [u8], bytes: Vec<u8>, max_size: u32) -> Cow<'a, [u8]> {
    if ensure_encoded_within_bound(label, bytes.len(), max_size) {
        Cow::Owned(bytes)
    } else {
        Cow::Owned(Vec::new())
    }
}

pub fn ensure_encoded_within_bound(label: &'static [u8], encoded_len: usize, max_size: u32) -> bool {
    if encoded_len > max_size as usize {
        record_corrupt(label);
        return false;
    }
    true
}

pub fn mark_decode_failure(label: &'static [u8], fail_closed: bool) {
    record_corrupt(label);
    if fail_closed {
        crate::meta::set_needs_migration(true);
    }
}
