//! どこで: chain_data 共通codec補助 / 何を: Bound防波堤とdecode方針補助 / なぜ: 破損時の扱いを統一するため

use crate::corrupt_log::record_corrupt;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncodeOverflow;

pub fn encode_guarded<'a>(
    label: &'static [u8],
    bytes: Cow<'a, [u8]>,
    max_size: u32,
) -> Result<Cow<'a, [u8]>, EncodeOverflow> {
    if !ensure_encoded_within_bound(label, bytes.len(), max_size) {
        return Err(EncodeOverflow);
    }
    Ok(bytes)
}

pub fn ensure_encoded_within_bound(
    label: &'static [u8],
    encoded_len: usize,
    max_size: u32,
) -> bool {
    if encoded_len > max_size as usize {
        record_corrupt(label);
    }
    encoded_len <= max_size as usize
}

pub fn mark_decode_failure(label: &'static [u8], fail_closed: bool) {
    record_corrupt(label);
    if fail_closed && is_fail_closed_label(label) {
        crate::meta::set_needs_migration(true);
    }
}

// fail-closed policy:
// - true is allowed only for core ledger integrity payloads.
// - config/ops/prune auxiliary payloads must stay fail-open to avoid whole-write freeze
//   on minor operational decode issues.
fn is_fail_closed_label(label: &'static [u8]) -> bool {
    label == b"state_root_meta"
        || label == b"receipt"
        || label == b"tx_loc"
        || label == b"tx_loc_kind"
        || label == b"block_data"
        || label == b"head"
        || label == b"tx_id"
        || label == b"stored_tx_decode"
        || label == b"tx_index"
}

#[cfg(test)]
mod tests {
    use super::{encode_guarded, mark_decode_failure};
    use crate::meta::needs_migration;
    use crate::stable_state::init_stable_state;
    use std::borrow::Cow;

    #[test]
    fn encode_guarded_accepts_borrowed() {
        let buf = [0x11u8; 4];
        let encoded = encode_guarded(b"borrowed", Cow::Borrowed(&buf), 4).expect("encode_guarded");
        assert_eq!(encoded.as_ref(), &buf);
    }

    #[test]
    fn fail_closed_unknown_label_does_not_set_needs_migration() {
        init_stable_state();
        assert!(!needs_migration());
        mark_decode_failure(b"ops_config", true);
        assert!(!needs_migration());
    }
}
