//! どこで: stable codec境界 / 何を: 固定長と可変長journalサイズ / なぜ: decode前条件を純粋判定にするため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec(matches => ensures
    matches == (actual_len == expected_len),
))]
pub fn fixed_len_matches(actual_len: usize, expected_len: usize) -> bool {
    actual_len == expected_len
}

#[cfg_attr(verus_keep_ghost, verus_spec(matches => ensures
    matches == (actual_len <= max_len),
))]
pub fn bounded_len(actual_len: usize, max_len: usize) -> bool {
    actual_len <= max_len
}

#[cfg_attr(verus_keep_ghost, verus_spec(converted => ensures
    len <= u32::MAX as usize ==> converted == Option::<u32>::Some(len as u32),
    len > u32::MAX as usize ==> converted == Option::<u32>::None,
))]
pub fn len_to_u32(len: usize) -> Option<u32> {
    u32::try_from(len).ok()
}

#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (status <= 1),
))]
pub fn valid_receipt_status(status: u8) -> bool {
    status <= 1
}

#[cfg_attr(verus_keep_ghost, verus_spec(len => ensures
    item_count > max_items ==> len == Option::<usize>::None,
    matches!(len, Some(_)) ==> item_count <= max_items,
    matches!(len, Some(_)) ==> len.unwrap() >= base_len,
))]
pub fn variable_items_len(
    base_len: usize,
    item_count: usize,
    item_len: usize,
    max_items: usize,
) -> Option<usize> {
    if item_count > max_items {
        return None;
    }
    base_len.checked_add(item_count.checked_mul(item_len)?)
}

#[cfg_attr(verus_keep_ghost, verus_spec(matches => ensures
    matches ==> item_count <= max_items,
))]
pub fn variable_items_len_matches(
    actual_len: usize,
    base_len: usize,
    item_count: usize,
    item_len: usize,
    max_items: usize,
) -> bool {
    variable_items_len(base_len, item_count, item_len, max_items) == Some(actual_len)
}

#[cfg_attr(verus_keep_ghost, verus_spec(len => ensures
    ptr_count > max_ptrs ==> len == Option::<usize>::None,
    matches!(len, Some(_)) ==> ptr_count <= max_ptrs,
    matches!(len, Some(_)) ==> len.unwrap() >= 4,
))]
pub fn prune_journal_len(ptr_count: u32, max_ptrs: u32) -> Option<usize> {
    if ptr_count > max_ptrs {
        return None;
    }
    let payload = usize::try_from(ptr_count).ok()?.checked_mul(20)?;
    4usize.checked_add(payload)
}

#[cfg_attr(verus_keep_ghost, verus_spec(matches => ensures
    matches ==> ptr_count <= max_ptrs,
    matches ==> actual_len >= 4,
))]
pub fn prune_journal_len_matches(actual_len: usize, ptr_count: u32, max_ptrs: u32) -> bool {
    prune_journal_len(ptr_count, max_ptrs) == Some(actual_len)
}

#[cfg(test)]
mod tests {
    use super::{
        bounded_len, fixed_len_matches, len_to_u32, prune_journal_len, prune_journal_len_matches,
        valid_receipt_status, variable_items_len, variable_items_len_matches,
    };

    #[test]
    fn fixed_len_requires_exact_length() {
        assert!(fixed_len_matches(32, 32));
        assert!(!fixed_len_matches(31, 32));
        assert!(bounded_len(10, 10));
        assert!(!bounded_len(11, 10));
        assert!(valid_receipt_status(1));
        assert!(!valid_receipt_status(2));
    }

    #[test]
    fn prune_journal_len_accounts_for_header_and_ptrs() {
        assert_eq!(prune_journal_len(0, 10), Some(4));
        assert_eq!(prune_journal_len(2, 10), Some(44));
        assert_eq!(prune_journal_len(11, 10), None);
        assert!(prune_journal_len_matches(44, 2, 10));
    }

    #[test]
    fn variable_items_len_checks_count_and_overflow() {
        assert_eq!(len_to_u32(7), Some(7));
        assert_eq!(variable_items_len(4, 2, 32, 10), Some(68));
        assert!(variable_items_len_matches(68, 4, 2, 32, 10));
        assert_eq!(variable_items_len(4, 11, 32, 10), None);
        assert_eq!(variable_items_len(usize::MAX, 1, 32, 10), None);
    }
}
