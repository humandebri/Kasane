    use super::{
        derive_caller_evm_cached, is_principal_decode_suppressed, note_decode_drop_for_principal,
        remaining_instruction_budget, should_stop_block_execution, store_internal_traces,
        CALLER_EVM_BY_PRINCIPAL, CALLER_EVM_CACHE_CAPACITY, DECODE_SUPPRESS_UNTIL_BY_PRINCIPAL,
    };
    use evm_db::chain_data::{InternalTrace, InternalTraceActionKind, InternalTraceSet, TxId};
    use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};
    use evm_db::Storable;
    use std::borrow::Cow;
    use std::collections::BTreeMap;

    #[test]
    fn instruction_budget_helpers_honor_gas_and_soft_limit_boundaries() {
        let stop_cases = [
            ("gas_at_limit", 10, 10, 0, 0, 0, true),
            ("gas_over_limit", 11, 10, 0, 0, 0, true),
            ("budget_at_limit", 1, 0, 5, 100, 105, true),
            ("budget_over_limit", 1, 0, 5, 100, 120, true),
            ("budget_under_limit", 1, 0, 5, 100, 104, false),
            ("budget_disabled", 1, 0, 0, 100, 100_000, false),
        ];
        for (case, block_gas_used, block_gas_limit, soft_limit, start, now, expected) in stop_cases
        {
            assert_eq!(
                should_stop_block_execution(
                    block_gas_used,
                    block_gas_limit,
                    soft_limit,
                    start,
                    now,
                ),
                expected,
                "{case}"
            );
        }

        let remaining_cases = [
            ("disabled", 0, 100, 1_000, None),
            ("remaining", 5, 100, 103, Some(2)),
            ("at_limit", 5, 100, 105, Some(0)),
            ("past_limit", 5, 100, 999, Some(0)),
        ];
        for (case, soft_limit, start, now, expected) in remaining_cases {
            assert_eq!(
                remaining_instruction_budget(soft_limit, start, now),
                expected,
                "{case}"
            );
        }
    }

    #[test]
    fn query_instruction_soft_limit_returns_stored_value() {
        let cases = [
            ("configured", 123_456, Some(123_456)),
            ("zero_stays_zero", 0, Some(0)),
        ];
        for (case, configured_limit, expected) in cases {
            init_stable_state();
            with_state_mut(|state| {
                let mut chain_state = *state.chain_state.get();
                chain_state.query_instruction_soft_limit = configured_limit;
                state.chain_state.set(chain_state);
            });
            assert_eq!(super::query_instruction_soft_limit(), expected, "{case}");
        }
    }

    #[test]
    fn decode_suppression_activates_after_threshold_and_expires() {
        DECODE_SUPPRESS_UNTIL_BY_PRINCIPAL.with(|cell| cell.borrow_mut().clear());
        let principal = b"p-1".to_vec();
        let now = 1_000u64;
        let mut per_block = BTreeMap::new();
        for _ in 0..evm_db::chain_data::DEFAULT_DECODE_SUPPRESS_STRIKES_PER_BLOCK {
            note_decode_drop_for_principal(principal.as_slice(), now, &mut per_block);
        }
        assert!(is_principal_decode_suppressed(principal.as_slice(), now));
        assert!(!is_principal_decode_suppressed(
            principal.as_slice(),
            now + evm_db::chain_data::DEFAULT_DECODE_SUPPRESS_WINDOW_SECS + 1
        ));
    }

    #[test]
    fn decode_suppression_map_is_bounded() {
        DECODE_SUPPRESS_UNTIL_BY_PRINCIPAL.with(|cell| cell.borrow_mut().clear());
        let now = 2_000u64;
        let mut per_block = BTreeMap::new();
        for i in 0..(evm_db::chain_data::DEFAULT_MAX_DECODE_SUPPRESS_PRINCIPALS + 128) {
            let principal = format!("principal-{i}").into_bytes();
            per_block.insert(
                principal.clone(),
                evm_db::chain_data::DEFAULT_DECODE_SUPPRESS_STRIKES_PER_BLOCK - 1,
            );
            note_decode_drop_for_principal(principal.as_slice(), now, &mut per_block);
        }
        DECODE_SUPPRESS_UNTIL_BY_PRINCIPAL.with(|cell| {
            assert!(
                cell.borrow().len() <= evm_db::chain_data::DEFAULT_MAX_DECODE_SUPPRESS_PRINCIPALS
            );
        });
    }

    #[test]
    fn caller_evm_cache_hits_for_same_principal() {
        CALLER_EVM_BY_PRINCIPAL.with(|cell| cell.borrow_mut().clear());
        let principal = b"cache-hit-principal";
        let first = derive_caller_evm_cached(principal).expect("first derive must succeed");
        let second = derive_caller_evm_cached(principal).expect("second derive must succeed");
        assert_eq!(first, second);
        CALLER_EVM_BY_PRINCIPAL.with(|cell| {
            assert_eq!(cell.borrow().len(), 1);
        });
    }

    #[test]
    fn caller_evm_cache_is_bounded() {
        CALLER_EVM_BY_PRINCIPAL.with(|cell| cell.borrow_mut().clear());
        for i in 0..(CALLER_EVM_CACHE_CAPACITY + 1) {
            let principal = format!("cache-cap-{i}").into_bytes();
            let _ = derive_caller_evm_cached(principal.as_slice()).expect("derive must succeed");
        }
        CALLER_EVM_BY_PRINCIPAL.with(|cell| {
            assert!(cell.borrow().len() <= CALLER_EVM_CACHE_CAPACITY);
        });
    }

    #[test]
    fn store_internal_traces_stores_failed_marker_on_encode_failure() {
        init_stable_state();
        let traces = InternalTraceSet::new(vec![InternalTrace {
            block_number: 1,
            tx_index: 0,
            trace_id: "x".repeat(65),
            depth: 1,
            action_kind: InternalTraceActionKind::Call,
            from_address: [0x11; 20],
            to_address: Some([0x22; 20]),
            value: [0x33; 32],
            created_contract_address: None,
            success: true,
            error_code: None,
        }]);
        let tx_id = TxId([0x44; 32]);
        let ptr = with_state_mut(|state| {
            store_internal_traces(state, tx_id, &traces)
                .expect("failed marker ptr must be returned")
        });
        with_state(|state| {
            let bytes = state
                .blob_store
                .read(&ptr)
                .expect("failed marker bytes must be readable");
            let decoded = InternalTraceSet::from_bytes(Cow::Owned(bytes));
            assert!(decoded.encode_failed);
            assert!(!decoded.truncated);
            assert_eq!(decoded.captured_count, 0);
            assert_eq!(decoded.total_count, 1);
            assert!(decoded.items.is_empty());
        });
    }
