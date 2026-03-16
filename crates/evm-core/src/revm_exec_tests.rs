    use super::{
        add_base_fee_portion_to_recipient, compute_effective_gas_price, map_halt_reason,
        validate_execution_result_sizes, ExecError, InternalTraceActionKind,
        InternalTraceInspector, LogEntry, OpHaltReason, StateDiff, FEE_RECIPIENT, MAX_RETURN_DATA,
    };
    use alloy_primitives::Bytes;
    use evm_db::stable_state::{init_stable_state, with_state_mut};
    use evm_db::types::keys::make_account_key;
    use evm_db::types::values::AccountVal;
    use revm::context_interface::result::{HaltReason, OutOfGasError};
    use revm::context_interface::CreateScheme;
    use revm::interpreter::{
        CallInput, CallInputs, CallOutcome, CallScheme, CallValue, CreateInputs, CreateOutcome, Gas,
        InstructionResult, InterpreterResult,
    };
    use revm::primitives::{Address, Bytes as RevmBytes, B256, U256};
    use revm::state::AccountInfo;

    #[test]
    fn effective_price_uses_min_of_max_and_base_plus_priority() {
        let effective = compute_effective_gas_price(10, 3, 5);
        assert_eq!(effective, Some(8));
        let effective = compute_effective_gas_price(7, 7, 0);
        assert_eq!(effective, Some(7));
    }

    #[test]
    fn effective_price_rejects_invalid_fees() {
        let effective = compute_effective_gas_price(10, 11, 0);
        assert_eq!(effective, None);
        let effective = compute_effective_gas_price(9, 0, 10);
        assert_eq!(effective, None);
    }

    #[test]
    fn effective_price_handles_overflow_without_panic() {
        let effective = compute_effective_gas_price(u128::MAX, u128::MAX, u64::MAX);
        assert_eq!(effective, None);
    }

    #[test]
    fn halt_reason_mapping_covers_known_variants() {
        assert_eq!(
            map_halt_reason(HaltReason::OutOfGas(OutOfGasError::Basic)),
            OpHaltReason::OutOfGas
        );
        assert_eq!(
            map_halt_reason(HaltReason::InvalidJump),
            OpHaltReason::InvalidJump
        );
    }

    #[test]
    fn validate_execution_result_sizes_rejects_large_return_data() {
        let output = vec![0u8; MAX_RETURN_DATA.saturating_add(1)];
        let logs: Vec<LogEntry> = Vec::new();
        let err = validate_execution_result_sizes(&output, &logs).expect_err("should fail");
        assert_eq!(err, ExecError::ResultTooLarge);
    }

    #[test]
    fn base_fee_credit_creates_recipient_when_missing_from_state_diff() {
        init_stable_state();
        let mut state = StateDiff::default();

        add_base_fee_portion_to_recipient(&mut state, 21_000, 1_000_000_000);

        let account = state.get(&FEE_RECIPIENT).expect("recipient must exist");
        let expected = u128::from(21_000u64).saturating_mul(u128::from(1_000_000_000u64));
        assert_eq!(account.info.balance, U256::from(expected));
        assert!(account.is_touched());
    }

    #[test]
    fn base_fee_credit_preserves_existing_nonce_and_code_hash() {
        init_stable_state();
        let recipient = FEE_RECIPIENT.into_array();
        let nonce = 7u64;
        let code_hash = [0xabu8; 32];
        let starting_balance = 123u128;
        with_state_mut(|state| {
            state.accounts.insert(
                make_account_key(recipient),
                AccountVal::from_parts(
                    nonce,
                    U256::from(starting_balance).to_be_bytes(),
                    code_hash,
                ),
            );
        });

        let mut state = StateDiff::default();
        add_base_fee_portion_to_recipient(&mut state, 30_000, 1_000_000_000);

        let account = state.get(&FEE_RECIPIENT).expect("recipient must exist");
        let reward = u128::from(30_000u64).saturating_mul(u128::from(1_000_000_000u64));
        assert_eq!(
            account.info.balance,
            U256::from(starting_balance.saturating_add(reward))
        );
        assert_eq!(account.info.nonce, nonce);
        assert_eq!(account.info.code_hash, B256::from(code_hash));
        assert!(account.is_touched());
    }

    #[test]
    fn base_fee_credit_keeps_selfdestruct_mark_on_recipient() {
        init_stable_state();
        let mut state = StateDiff::default();
        let mut account = revm::state::Account::from(AccountInfo::default());
        account.mark_selfdestruct();
        state.insert(FEE_RECIPIENT, account);

        add_base_fee_portion_to_recipient(&mut state, 21_000, 1_000_000_000);

        let updated = state.get(&FEE_RECIPIENT).expect("recipient must exist");
        assert!(updated.is_selfdestructed());
    }

    #[test]
    fn internal_trace_inspector_marks_custom_create_explicitly() {
        let caller = Address::with_last_byte(0x11);
        let created = Address::with_last_byte(0x22);
        let target = Address::with_last_byte(0x33);
        let mut inspector = InternalTraceInspector::new(7, 3);
        let outer = CallInputs {
            input: CallInput::Bytes(RevmBytes::default()),
            return_memory_offset: 0..0,
            gas_limit: 100_000,
            bytecode_address: target,
            known_bytecode: None,
            target_address: target,
            caller,
            value: CallValue::Transfer(U256::ZERO),
            scheme: CallScheme::Call,
            is_static: false,
        };
        inspector.start_call(&outer);
        let inputs = CreateInputs::new(
            caller,
            CreateScheme::Custom { address: created },
            U256::ZERO,
            Bytes::default(),
            100_000,
        );
        inspector.start_create(&inputs);
        let outcome = CreateOutcome::new(
            InterpreterResult::new(
                InstructionResult::Return,
                Bytes::default(),
                Gas::new(100_000),
            ),
            Some(created),
        );
        inspector.end_create(&outcome);

        let traces = inspector.finish();
        assert_eq!(traces.items.len(), 1);
        assert_eq!(traces.total_count, 1);
        assert_eq!(traces.items[0].action_kind, InternalTraceActionKind::Custom);
        assert_eq!(
            traces.items[0].created_contract_address,
            Some(created.into_array())
        );
    }

    #[test]
    fn internal_trace_selfdestruct_does_not_shift_sibling_trace_ids() {
        let caller = Address::with_last_byte(0x11);
        let target = Address::with_last_byte(0x22);
        let created = Address::with_last_byte(0x33);
        let sibling_target = Address::with_last_byte(0x66);
        let mut inspector = InternalTraceInspector::new(9, 4);
        let outer = CallInputs {
            input: CallInput::Bytes(RevmBytes::default()),
            return_memory_offset: 0..0,
            gas_limit: 100_000,
            bytecode_address: target,
            known_bytecode: None,
            target_address: target,
            caller,
            value: CallValue::Transfer(U256::ZERO),
            scheme: CallScheme::Call,
            is_static: false,
        };
        inspector.start_call(&outer);
        let inner = CallInputs {
            target_address: Address::with_last_byte(0x44),
            ..outer.clone()
        };
        inspector.start_call(&inner);
        inspector.record_selfdestruct(inner.target_address, Address::with_last_byte(0x55), U256::from(7));
        let create = CreateInputs::new(
            inner.target_address,
            CreateScheme::Create,
            U256::from(9),
            Bytes::default(),
            100_000,
        );
        inspector.start_create(&create);
        let outcome = CreateOutcome::new(
            InterpreterResult::new(
                InstructionResult::Return,
                Bytes::default(),
                Gas::new(100_000),
            ),
            Some(created),
        );
        inspector.end_create(&outcome);
        let sibling_call = CallInputs {
            target_address: sibling_target,
            bytecode_address: sibling_target,
            ..outer.clone()
        };
        inspector.start_call(&sibling_call);
        inspector.end_call(&CallOutcome::new(
            InterpreterResult::new(
                InstructionResult::Return,
                Bytes::default(),
                Gas::new(100_000),
            ),
            0..0,
        ));
        inspector.end_call(&CallOutcome::new(
            InterpreterResult::new(
                InstructionResult::Return,
                Bytes::default(),
                Gas::new(100_000),
            ),
            0..0,
        ));

        let traces = inspector.finish();
        assert_eq!(traces.total_count, 4);
        assert_eq!(traces.items.len(), 4);
        assert_eq!(traces.items[0].trace_id, "0");
        assert_eq!(traces.items[1].trace_id, "0_0");
        assert_eq!(traces.items[1].action_kind, InternalTraceActionKind::Selfdestruct);
        assert_eq!(traces.items[2].trace_id, "0_1");
        assert_eq!(traces.items[2].action_kind, InternalTraceActionKind::Create);
        assert_eq!(
            traces.items[2].created_contract_address,
            Some(created.into_array())
        );
        assert_eq!(traces.items[3].trace_id, "0_2");
        assert_eq!(traces.items[3].action_kind, InternalTraceActionKind::Call);
        assert_eq!(traces.items[3].to_address, Some(sibling_target.into_array()));
    }