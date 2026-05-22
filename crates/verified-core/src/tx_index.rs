//! どこで: tx id/index整合 / 何を: block内indexとTxLocの純粋判定 / なぜ: 永続化前の位置情報を証明対象にするため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const TX_LOC_KIND_INCLUDED: u8 = 1;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IncludedTxPosition {
    pub block_number: u64,
    pub tx_index: u32,
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    included_len <= u32::MAX as usize ==> result == Option::<u32>::Some(included_len as u32),
    included_len > u32::MAX as usize ==> result == Option::<u32>::None,
))]
pub fn next_included_index(included_len: usize) -> Option<u32> {
    u32::try_from(included_len).ok()
}

#[cfg_attr(verus_keep_ghost, verus_spec(matches => ensures
    matches == (
        loc_kind == TX_LOC_KIND_INCLUDED
        && loc_block_number == position.block_number
        && loc_tx_index == position.tx_index
    ),
))]
pub fn included_position_matches(
    position: IncludedTxPosition,
    loc_kind: u8,
    loc_block_number: u64,
    loc_tx_index: u32,
) -> bool {
    loc_kind == TX_LOC_KIND_INCLUDED
        && loc_block_number == position.block_number
        && loc_tx_index == position.tx_index
}

#[cfg_attr(verus_keep_ghost, verus_spec(matches => ensures
    matches == (
        loc_kind == TX_LOC_KIND_INCLUDED
        && loc_block_number == entry.block_number
        && loc_tx_index == entry.tx_index
    ),
))]
pub fn tx_index_entry_matches_loc(
    entry: IncludedTxPosition,
    loc_kind: u8,
    loc_block_number: u64,
    loc_tx_index: u32,
) -> bool {
    included_position_matches(entry, loc_kind, loc_block_number, loc_tx_index)
}

#[cfg(test)]
mod tests {
    use super::{
        included_position_matches, next_included_index, tx_index_entry_matches_loc,
        IncludedTxPosition, TX_LOC_KIND_INCLUDED,
    };

    #[test]
    fn next_included_index_rejects_u32_overflow() {
        assert_eq!(next_included_index(0), Some(0));
        assert_eq!(next_included_index(u32::MAX as usize), Some(u32::MAX));
        assert_eq!(next_included_index((u32::MAX as usize) + 1), None);
    }

    #[test]
    fn included_position_requires_kind_block_and_index_match() {
        let position = IncludedTxPosition {
            block_number: 42,
            tx_index: 7,
        };
        assert!(included_position_matches(
            position,
            TX_LOC_KIND_INCLUDED,
            42,
            7
        ));
        assert!(!included_position_matches(position, 0, 42, 7));
        assert!(!included_position_matches(
            position,
            TX_LOC_KIND_INCLUDED,
            41,
            7
        ));
        assert!(!tx_index_entry_matches_loc(
            position,
            TX_LOC_KIND_INCLUDED,
            42,
            8
        ));
    }
}
