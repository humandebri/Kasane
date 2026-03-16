    use super::{apply_journal, AnchorDelta, JournalUpdate, NewNodeRecords, NodeDeltaCounts};
    use evm_db::chain_data::HashKey;
    use evm_db::meta::{clear_needs_migration, needs_migration};
    use evm_db::stable_state::{init_stable_state, with_state_mut};
    use std::collections::BTreeMap;

    #[test]
    fn inconsistent_journal_marks_decode_failure_instead_of_panicking() {
        init_stable_state();
        clear_needs_migration();

        with_state_mut(|state| {
            let mut deltas: NodeDeltaCounts = BTreeMap::new();
            let inconsistent = HashKey([0x11u8; 32]);
            deltas.insert(inconsistent, 1);
            apply_journal(
                state,
                JournalUpdate {
                    node_delta_counts: deltas,
                    new_node_records: NewNodeRecords::new(),
                    anchor_delta: AnchorDelta::default(),
                },
            );
            assert!(state.state_root_node_db.get(&inconsistent).is_none());
        });

        assert!(needs_migration());
    }