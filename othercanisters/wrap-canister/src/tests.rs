use super::{
    apply_insert_request_outcome, apply_runtime_config, approval_required_for_readiness,
    decode_asset_decimals, decode_stored_request, decode_u256_be, dequeue_request,
    derive_wrap_request_id, encode_factory_mint_for_asset_call_data, encode_stored_request,
    enqueue_request, init_state, insert_request, insert_wrap_request, is_withdrawable,
    map_transfer_reply, mark_request_running, mark_wrap_request_running, nat_from_32_be,
    nat_to_be_bytes, native_withdraw_receive_amount, normalize_submit_wrap_args,
    on_worker_queue_drain, on_wrap_worker_queue_drain, principal_from_bytes,
    recover_request_state_after_upgrade, recover_wrap_request_state_after_upgrade, schedule_worker,
    schedule_wrap_worker, submit_error_to_code, to_request_id, to_withdraw_error_code,
    transfer_error_to_code, transfer_from_error_to_code, u256_from_u64,
    validate_non_anonymous_principal, validate_quote_within_approval, validate_runtime_config,
    validate_withdraw_request, with_state, with_state_mut, FeeCharge, GetUnwrapRequirementsArgs,
    Icrc1MetadataValue, Icrc1TransferError, Icrc2TransferFromError, InitArgs, InsertRequestOutcome,
    NormalizedDispatchUnwrapRequest, NormalizedSubmitWrapRequest, QueueMeta, RequestResult,
    RequestStatus, StoredRequest, SubmitTxError, SubmitWrapRequestArgs, UnwrapReadiness, WrapQuote,
    WrapRequestResult, WrapStoredRequest, WORKER_SCHEDULED, WRAP_WORKER_SCHEDULED,
};
use candid::{decode_one, encode_one, Nat, Principal};
use ic_evm_rpc_types::ApiError;
use num_bigint::BigUint;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

fn run_ready_future<F>(future: F) -> F::Output
where
    F: Future,
{
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = pin!(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("test future must complete without suspension"),
    }
}

fn reset_state() {
    init_state();
    with_state_mut(|state| {
        let allowed_asset_keys: Vec<_> = state
            .allowed_assets
            .range(..)
            .map(|entry| entry.key().clone())
            .collect();
        for key in allowed_asset_keys {
            state.allowed_assets.remove(&key);
        }
        state.allowed_assets.insert(vec![2u8; 29], 1);
        state.allowed_assets.insert(vec![7u8; 29], 1);
        let request_keys: Vec<_> = state.requests.range(..).map(|entry| *entry.key()).collect();
        for key in request_keys {
            state.requests.remove(&key);
        }
        let queue_keys: Vec<_> = state.queue.range(..).map(|entry| *entry.key()).collect();
        for key in queue_keys {
            state.queue.remove(&key);
        }
        let wrap_request_keys: Vec<_> = state
            .wrap_requests
            .range(..)
            .map(|entry| *entry.key())
            .collect();
        for key in wrap_request_keys {
            state.wrap_requests.remove(&key);
        }
        let wrap_queue_keys: Vec<_> = state
            .wrap_queue
            .range(..)
            .map(|entry| *entry.key())
            .collect();
        for key in wrap_queue_keys {
            state.wrap_queue.remove(&key);
        }
        let _ = state.queue_meta.set(super::QueueMeta::new());
        let _ = state.wrap_queue_meta.set(super::QueueMeta::new());
        let _ = state.kasane_canister.set(Vec::new());
        let _ = state.evm_gateway_canister.set(Vec::new());
        let _ = state.native_ledger_canister.set(Vec::new());
        let _ = state.fee_policy.set(super::FeePolicyStored {
            fee_ledger_canister: Vec::new(),
            cycle_fee_e8s: super::DEFAULT_CYCLE_FEE_E8S,
            gas_price_buffer_bps: super::DEFAULT_GAS_PRICE_BUFFER_BPS,
        });
        let _ = state.wrap_evm_config.set(super::WrapEvmConfigStored {
            wrap_factory_address: Vec::new(),
        });
    });
    super::PENDING_WRAP_SUBMISSIONS.with(|pending| {
        pending.borrow_mut().clear();
    });
}

fn sample_init_args(seed: u8, factory: [u8; 20]) -> InitArgs {
    InitArgs {
        kasane_canister: Principal::self_authenticating([seed, 1]),
        evm_gateway_canister: Principal::self_authenticating([seed, 2]),
        fee_ledger_canister: Principal::self_authenticating([seed, 3]),
        native_ledger_canister: Principal::self_authenticating([seed, 5]),
        wrap_factory_address: factory.to_vec(),
        cycle_fee_e8s: u64::from(seed) + 1_000,
        gas_price_buffer_bps: 12_000 + u32::from(seed),
        allowed_assets: vec![Principal::self_authenticating([seed, 4])],
    }
}

fn no_schedule() {}

fn test_fee_charge() -> FeeCharge {
    FeeCharge {
        ledger_tx_id: vec![0x44, 0x55],
        charged_fee_e8s: 1_000_000,
        charged_gas_price_wei: 300_000_000_000,
    }
}

fn test_fee_ledger() -> Principal {
    Principal::self_authenticating(b"wrap-fee-ledger")
}

fn sample_unwrap_args(request_id: [u8; 32]) -> NormalizedDispatchUnwrapRequest {
    NormalizedDispatchUnwrapRequest {
        request_id: request_id.to_vec(),
        asset_id: vec![2u8; 29],
        amount: vec![0u8; 32],
        recipient: vec![3u8; 29],
    }
}

fn sample_normalized_wrap_request() -> NormalizedSubmitWrapRequest {
    NormalizedSubmitWrapRequest {
        request_id: to_request_id(&[0x11; 32]).expect("id"),
        asset_id: vec![2u8; 29],
        amount: vec![0u8; 32],
        evm_recipient: vec![4u8; 20],
        gas_limit: 200_000,
        max_fee_e8s: 1_000_000,
        quoted_gas_price_wei: 300_000_000_000,
        fee_ledger_canister: test_fee_ledger(),
    }
}

fn sample_wrap_quote() -> WrapQuote {
    WrapQuote {
        charged_fee_e8s: 1_000_000,
        charged_gas_price_wei: 300_000_000_000,
        cycle_fee_e8s: super::DEFAULT_CYCLE_FEE_E8S,
        fee_ledger_canister: test_fee_ledger(),
    }
}

fn sample_request_result(status: RequestStatus) -> RequestResult {
    RequestResult {
        status,
        ledger_tx_id: None,
        error_code: None,
        dispatch_status: Some(super::RequestDispatchStatusView::Dispatched),
        dispatch_error: None,
    }
}

fn sample_stored_request(status: RequestStatus) -> StoredRequest {
    StoredRequest {
        asset_id: vec![2u8; 29],
        amount: vec![0u8; 32],
        recipient: vec![3u8; 29],
        created_at_time: 1,
        result: sample_request_result(status),
    }
}

fn sample_failed_unwrap_request_for(recipient: Principal) -> StoredRequest {
    StoredRequest {
        asset_id: vec![2u8; 29],
        amount: vec![0u8; 32],
        recipient: recipient.as_slice().to_vec(),
        created_at_time: 1,
        result: RequestResult {
            status: RequestStatus::Failed,
            ledger_tx_id: None,
            error_code: Some("ledger.call_failed:oops".to_string()),
            dispatch_status: Some(super::RequestDispatchStatusView::Dispatched),
            dispatch_error: None,
        },
    }
}

#[test]
fn icrc10_supported_standards_advertise_icrc21() {
    let standards = super::icrc21::supported_standards();
    assert!(standards.iter().any(|item| item.name == "ICRC-21"));
}

#[test]
fn icrc21_retry_request_rejects_missing_request() {
    reset_state();
    let response = run_ready_future(super::icrc21::consent_message(
        super::icrc21::Icrc21ConsentMessageRequest {
            method: "retry_request".to_string(),
            arg: encode_one(super::RetryRequestArgs {
                request_id: vec![0x55; 32],
            })
            .expect("encode retry args"),
            user_preferences: super::icrc21::Icrc21ConsentMessageSpec {
                metadata: super::icrc21::Icrc21ConsentMessageMetadata {
                    utc_offset_minutes: Some(540),
                    language: "en".to_string(),
                },
                device_spec: None,
            },
        },
    ));
    assert!(matches!(
        response,
        Err(super::icrc21::Icrc21Error::ConsentMessageUnavailable(_))
    ));
}

#[test]
fn icrc21_recover_failed_wrap_rejects_missing_request() {
    reset_state();
    let response = run_ready_future(super::icrc21::consent_message(
        super::icrc21::Icrc21ConsentMessageRequest {
            method: "recover_failed_wrap".to_string(),
            arg: encode_one(super::RecoverFailedWrapArgs {
                request_id: vec![0x77; 32],
            })
            .expect("encode recover args"),
            user_preferences: super::icrc21::Icrc21ConsentMessageSpec {
                metadata: super::icrc21::Icrc21ConsentMessageMetadata {
                    utc_offset_minutes: Some(540),
                    language: "en".to_string(),
                },
                device_spec: None,
            },
        },
    ));
    assert!(matches!(
        response,
        Err(super::icrc21::Icrc21Error::ConsentMessageUnavailable(_))
    ));
}

#[test]
fn icrc21_submit_wrap_request_rejects_when_quote_unavailable() {
    reset_state();
    let response = run_ready_future(super::icrc21::consent_message(
        super::icrc21::Icrc21ConsentMessageRequest {
            method: "submit_wrap_request".to_string(),
            arg: encode_one(super::SubmitWrapRequestArgs {
                asset_id: Principal::self_authenticating([2u8; 32]),
                amount_e8s: Nat::from(10_000_000u64),
                evm_recipient: vec![0x11; 20],
                evm_nonce: 7,
                gas_limit: 210_000,
                max_fee_e8s: Nat::from(1_000_000u64),
                quoted_gas_price_wei: Nat::from(300_000_000_000u64),
                fee_ledger_canister: test_fee_ledger(),
            })
            .expect("encode submit args"),
            user_preferences: super::icrc21::Icrc21ConsentMessageSpec {
                metadata: super::icrc21::Icrc21ConsentMessageMetadata {
                    utc_offset_minutes: Some(540),
                    language: "en".to_string(),
                },
                device_spec: None,
            },
        },
    ));
    assert!(matches!(
        response,
        Err(super::icrc21::Icrc21Error::ConsentMessageUnavailable(_))
    ));
}

#[test]
fn icrc21_submit_wrap_request_accepts_line_display_request_shape() {
    reset_state();
    let response = run_ready_future(super::icrc21::consent_message(
        super::icrc21::Icrc21ConsentMessageRequest {
            method: "submit_wrap_request".to_string(),
            arg: encode_one(super::SubmitWrapRequestArgs {
                asset_id: Principal::self_authenticating([2u8; 32]),
                amount_e8s: Nat::from(10_000_000u64),
                evm_recipient: vec![0x11; 20],
                evm_nonce: 7,
                gas_limit: 210_000,
                max_fee_e8s: Nat::from(1_000_000u64),
                quoted_gas_price_wei: Nat::from(300_000_000_000u64),
                fee_ledger_canister: test_fee_ledger(),
            })
            .expect("encode submit args"),
            user_preferences: super::icrc21::Icrc21ConsentMessageSpec {
                metadata: super::icrc21::Icrc21ConsentMessageMetadata {
                    utc_offset_minutes: Some(540),
                    language: "en".to_string(),
                },
                device_spec: Some(super::icrc21::Icrc21DeviceSpec::LineDisplay(
                    super::icrc21::Icrc21LineDisplaySpec {
                        characters_per_line: 24,
                        lines_per_page: 4,
                    },
                )),
            },
        },
    ));
    assert!(matches!(
        response,
        Err(super::icrc21::Icrc21Error::ConsentMessageUnavailable(_))
    ));
}

#[test]
fn wrap_did_does_not_export_fields_display_icrc21_shape() {
    let did = include_str!("../wrap_canister.did");
    assert!(!did.contains("FieldsDisplay"));
    assert!(did.contains("LineDisplay"));
}

#[test]
fn nat_from_32_be_keeps_high_bits() {
    let mut amount = [0u8; 32];
    amount[0] = 1;
    let nat = nat_from_32_be(&amount).expect("valid");
    assert_eq!(nat.0.bits(), 249);
}

#[test]
fn principal_from_bytes_rejects_too_long() {
    let err = principal_from_bytes(&[7u8; 30]).expect_err("must reject");
    assert_eq!(err, "arg.principal_invalid");
}

#[test]
fn validate_non_anonymous_principal_rejects_anonymous() {
    let err =
        validate_non_anonymous_principal(&Principal::anonymous(), "arg.kasane_canister_anonymous")
            .expect_err("must reject anonymous");
    assert_eq!(err, "arg.kasane_canister_anonymous");

    let err = validate_non_anonymous_principal(
        &Principal::anonymous(),
        "arg.fee_ledger_canister_anonymous",
    )
    .expect_err("must reject anonymous");
    assert_eq!(err, "arg.fee_ledger_canister_anonymous");
}

#[test]
fn apply_runtime_config_overwrites_all_runtime_settings() {
    reset_state();
    apply_runtime_config(sample_init_args(1, [0x11; 20]));
    apply_runtime_config(sample_init_args(9, [0x99; 20]));

    let (kasane, gateway, fee_ledger, cycle_fee, gas_buffer, factory) = with_state(|state| {
        let fee_policy = state.fee_policy.get().clone();
        let wrap_config = state.wrap_evm_config.get().clone();
        (
            principal_from_bytes(state.kasane_canister.get()).expect("kasane principal"),
            principal_from_bytes(state.evm_gateway_canister.get()).expect("gateway principal"),
            principal_from_bytes(fee_policy.fee_ledger_canister.as_slice())
                .expect("fee ledger principal"),
            fee_policy.cycle_fee_e8s,
            fee_policy.gas_price_buffer_bps,
            wrap_config.wrap_factory_address,
        )
    });

    let expected = sample_init_args(9, [0x99; 20]);
    assert_eq!(kasane, expected.kasane_canister);
    assert_eq!(gateway, expected.evm_gateway_canister);
    assert_eq!(fee_ledger, expected.fee_ledger_canister);
    assert_eq!(cycle_fee, expected.cycle_fee_e8s);
    assert_eq!(gas_buffer, expected.gas_price_buffer_bps);
    assert_eq!(factory, expected.wrap_factory_address);
}

#[test]
fn replace_allowed_assets_overwrites_previous_entries() {
    reset_state();
    with_state_mut(|state| {
        super::replace_allowed_assets(
            state,
            &[
                Principal::self_authenticating(b"asset-a"),
                Principal::self_authenticating(b"asset-b"),
            ],
        )
        .expect("replace");
    });

    let allowed = super::allowed_assets_view().expect("view");
    assert_eq!(
        allowed,
        vec![
            Principal::self_authenticating(b"asset-a"),
            Principal::self_authenticating(b"asset-b"),
        ]
    );
    assert!(
        super::ensure_asset_allowed(Principal::self_authenticating(b"asset-a").as_slice()).is_ok()
    );
    assert_eq!(
        super::ensure_asset_allowed(Principal::self_authenticating(b"asset-c").as_slice()),
        Err("asset.not_allowed".to_string())
    );
}

#[test]
fn native_ledger_cannot_be_allowed_wrap_asset() {
    reset_state();
    let native = Principal::self_authenticating(b"native-ledger");
    let mut args = sample_init_args(1, [0x11; 20]);
    args.native_ledger_canister = native;
    args.allowed_assets = vec![native];

    let err = validate_runtime_config(&args).expect_err("native ICP must not be wrappable");
    assert_eq!(err, "asset.native_ledger_not_wrappable");

    apply_runtime_config(sample_init_args(2, [0x22; 20]));
    with_state_mut(|state| {
        let native = principal_from_bytes(state.native_ledger_canister.get()).expect("native");
        let err = super::replace_allowed_assets(state, &[native]).expect_err("reject native");
        assert_eq!(err, "asset.native_ledger_not_wrappable");
    });
}

#[test]
fn request_memo_is_stable_fixed_length_and_kind_sensitive() {
    let stable_id = to_request_id(&[0x33u8; 32]).expect("id");
    let first = super::request_memo(stable_id, super::TransferMemoKind::Unwrap);
    let second = super::request_memo(stable_id, super::TransferMemoKind::Unwrap);
    assert_eq!(first.len(), 32);
    assert_eq!(first, second);

    let diff_id = to_request_id(&[0x22u8; 32]).expect("id");
    let kinds = [
        super::TransferMemoKind::Unwrap,
        super::TransferMemoKind::Fee,
        super::TransferMemoKind::Pull,
        super::TransferMemoKind::Withdraw,
    ];
    let mut memos = Vec::new();
    for kind in kinds {
        let memo = super::request_memo(diff_id, kind);
        assert_eq!(memo.len(), 32);
        memos.push(memo);
    }
    for i in 0..memos.len() {
        for j in (i + 1)..memos.len() {
            assert_ne!(memos[i], memos[j], "memo kind pair {i}-{j}");
        }
    }
}

#[test]
fn insert_request_is_idempotent_for_same_payload() {
    reset_state();
    let args = sample_unwrap_args([1u8; 32]);
    let first = insert_request(args.clone()).expect("first should pass");
    assert_eq!(
        first,
        InsertRequestOutcome::Inserted(to_request_id(&[1u8; 32]).expect("id"))
    );
    let second = insert_request(args).expect("second should be idempotent");
    assert_eq!(
        second,
        InsertRequestOutcome::AlreadyExists(to_request_id(&[1u8; 32]).expect("id"))
    );
    let status = with_state(|state| {
        state
            .requests
            .get(&to_request_id(&[1u8; 32]).expect("id"))
            .map(|r| r.result.status)
    });
    assert_eq!(status, Some(RequestStatus::Queued));
    assert_eq!(with_state(|state| state.requests.len()), 1);
}

#[test]
fn insert_request_rejects_duplicate_when_payload_differs() {
    let cases = [
        (
            "asset_mismatch",
            NormalizedDispatchUnwrapRequest {
                asset_id: vec![9u8; 29],
                ..sample_unwrap_args([1u8; 32])
            },
        ),
        (
            "amount_mismatch",
            NormalizedDispatchUnwrapRequest {
                amount: vec![8u8; 32],
                ..sample_unwrap_args([1u8; 32])
            },
        ),
        (
            "recipient_mismatch",
            NormalizedDispatchUnwrapRequest {
                recipient: vec![7u8; 29],
                ..sample_unwrap_args([1u8; 32])
            },
        ),
    ];
    for (case, candidate) in cases {
        reset_state();
        insert_request(sample_unwrap_args([1u8; 32])).expect("first should pass");
        let err = insert_request(candidate).expect_err(case);
        assert_eq!(err, "request.idempotency_mismatch", "{case}");
    }
}

#[test]
fn submit_unwrap_request_does_not_requeue_existing_request() {
    reset_state();
    with_state_mut(|state| {
        let _ = state
            .kasane_canister
            .set(Principal::anonymous().as_slice().to_vec());
        let request_id = to_request_id(&[1u8; 32]).expect("id");
        state
            .requests
            .insert(request_id, sample_stored_request(RequestStatus::Queued));
        let mut meta = *state.queue_meta.get();
        let seq = meta.tail;
        meta.tail = meta.tail.saturating_add(1);
        state.queue.insert(seq, request_id);
        state.queue_meta.set(meta);
    });

    let request_id = apply_insert_request_outcome(
        InsertRequestOutcome::AlreadyExists(to_request_id(&[1u8; 32]).expect("id")),
        no_schedule,
    );
    assert_eq!(request_id, to_request_id(&[1u8; 32]).expect("id"));
    with_state(|state| {
        assert_eq!(state.requests.len(), 1);
        assert_eq!(state.queue.len(), 1);
        let req = state
            .requests
            .get(&to_request_id(&[1u8; 32]).expect("id"))
            .expect("request");
        assert_eq!(req.result.status, RequestStatus::Queued);
    });
}

#[test]
fn apply_insert_request_outcome_enqueues_new_request_once() {
    reset_state();
    let request_id = to_request_id(&[9u8; 32]).expect("id");
    with_state_mut(|state| {
        state
            .requests
            .insert(request_id, sample_stored_request(RequestStatus::Queued));
    });

    let returned =
        apply_insert_request_outcome(InsertRequestOutcome::Inserted(request_id), no_schedule);
    assert_eq!(returned, request_id);
    with_state(|state| {
        assert_eq!(state.requests.len(), 1);
        assert_eq!(state.queue.len(), 1);
    });
}

#[test]
fn submit_unwrap_request_keeps_queue_size_for_all_existing_statuses() {
    let statuses = [
        RequestStatus::Queued,
        RequestStatus::Running,
        RequestStatus::Succeeded,
        RequestStatus::Failed,
    ];
    for status in statuses {
        reset_state();
        with_state_mut(|state| {
            let _ = state
                .kasane_canister
                .set(Principal::anonymous().as_slice().to_vec());
            let request_id = to_request_id(&[status as u8 + 1; 32]).expect("id");
            state
                .requests
                .insert(request_id, sample_stored_request(status));
            if status == RequestStatus::Queued {
                let mut meta = *state.queue_meta.get();
                let seq = meta.tail;
                meta.tail = meta.tail.saturating_add(1);
                state.queue.insert(seq, request_id);
                state.queue_meta.set(meta);
            }
        });

        let request_id = apply_insert_request_outcome(
            InsertRequestOutcome::AlreadyExists(
                to_request_id(&[status as u8 + 1; 32]).expect("id"),
            ),
            no_schedule,
        );
        assert_eq!(
            request_id,
            to_request_id(&[status as u8 + 1; 32]).expect("id")
        );
        with_state(|state| {
            assert_eq!(state.requests.len(), 1, "{status:?}");
            let expected_queue_len = u64::from(status == RequestStatus::Queued);
            assert_eq!(state.queue.len(), expected_queue_len, "{status:?}");
            let req = state
                .requests
                .get(&to_request_id(&[status as u8 + 1; 32]).expect("id"))
                .expect("request");
            assert_eq!(req.result.status, status, "{status:?}");
        });
    }
}

#[test]
fn stored_request_codec_roundtrips_and_rejects_invalid_shapes() {
    let req = StoredRequest {
        asset_id: vec![1u8; 29],
        amount: vec![2u8; 32],
        recipient: vec![3u8; 29],
        created_at_time: 1,
        result: RequestResult {
            status: RequestStatus::Succeeded,
            ledger_tx_id: Some(vec![4u8; 16]),
            error_code: None,
            dispatch_status: Some(super::RequestDispatchStatusView::Dispatched),
            dispatch_error: None,
        },
    };
    let encoded = encode_stored_request(&req).expect("encode");
    let decoded = decode_stored_request(&encoded).expect("decode");
    assert_eq!(decoded.asset_id, req.asset_id);
    assert_eq!(decoded.amount, req.amount);
    assert_eq!(decoded.recipient, req.recipient);
    assert_eq!(decoded.result.status, RequestStatus::Succeeded);
    assert_eq!(decoded.result.ledger_tx_id, Some(vec![4u8; 16]));

    let invalid_req = StoredRequest {
        asset_id: vec![1u8; 30],
        amount: vec![2u8; 32],
        recipient: vec![3u8; 29],
        created_at_time: 1,
        result: sample_request_result(RequestStatus::Queued),
    };
    assert!(encode_stored_request(&invalid_req).is_none());
    assert!(decode_stored_request(&[0xFF]).is_none());
}

#[test]
fn recover_request_state_after_upgrade_requeues_running_request() {
    reset_state();
    let request_id = to_request_id(&[0x31u8; 32]).expect("id");
    with_state_mut(|state| {
        state
            .requests
            .insert(request_id, sample_stored_request(RequestStatus::Running));
        let mut meta = *state.queue_meta.get();
        let seq = meta.tail;
        meta.tail = meta.tail.saturating_add(1);
        state.queue.insert(seq, request_id);
        state.queue_meta.set(meta);
    });

    assert!(recover_request_state_after_upgrade(123));

    with_state(|state| {
        assert_eq!(state.queue.len(), 1);
        assert_eq!(state.queue_meta.get().head, 0);
        assert_eq!(state.queue_meta.get().tail, 1);
        assert_eq!(
            state.requests.get(&request_id).map(|req| req.result.status),
            Some(RequestStatus::Queued)
        );
        assert_eq!(state.queue.get(&0), Some(request_id));
    });
}

#[test]
fn recover_request_state_after_upgrade_fills_missing_queued_request_once() {
    reset_state();
    let request_id = to_request_id(&[0x32u8; 32]).expect("id");
    with_state_mut(|state| {
        state
            .requests
            .insert(request_id, sample_stored_request(RequestStatus::Queued));
    });

    assert!(recover_request_state_after_upgrade(123));
    assert!(recover_request_state_after_upgrade(124));
    with_state(|state| {
        assert_eq!(state.queue.len(), 1);
        assert_eq!(state.queue_meta.get().tail, 1);
        assert_eq!(state.queue.get(&0), Some(request_id));
    });
}

#[test]
fn recover_request_state_after_upgrade_keeps_terminal_requests_out_of_queue() {
    reset_state();
    let succeeded = to_request_id(&[0x33u8; 32]).expect("id");
    let failed = to_request_id(&[0x34u8; 32]).expect("id");
    with_state_mut(|state| {
        let mut succeeded_req = sample_stored_request(RequestStatus::Succeeded);
        succeeded_req.result.ledger_tx_id = Some(vec![1u8; 2]);
        state.requests.insert(succeeded, succeeded_req);

        let mut failed_req = sample_stored_request(RequestStatus::Failed);
        failed_req.result.error_code = Some("ledger.call_failed:oops".to_string());
        state.requests.insert(failed, failed_req);
    });

    assert!(!recover_request_state_after_upgrade(123));
    with_state(|state| {
        assert_eq!(state.queue.len(), 0);
        assert_eq!(
            state.requests.get(&succeeded).map(|req| req.result.status),
            Some(RequestStatus::Succeeded)
        );
        assert_eq!(
            state.requests.get(&failed).map(|req| req.result.status),
            Some(RequestStatus::Failed)
        );
    });
}

#[test]
fn queue_dequeue_preserves_order() {
    reset_state();
    let a = to_request_id(&[1u8; 32]).expect("id");
    let b = to_request_id(&[2u8; 32]).expect("id");
    enqueue_request(a);
    enqueue_request(b);
    assert_eq!(dequeue_request(), Some(a));
    assert_eq!(dequeue_request(), Some(b));
    assert_eq!(dequeue_request(), None);
}

#[test]
fn mark_request_running_sets_running_status() {
    reset_state();
    let request_id = to_request_id(&[1u8; 32]).expect("id");
    insert_request(NormalizedDispatchUnwrapRequest {
        amount: vec![3u8; 32],
        recipient: vec![4u8; 29],
        ..sample_unwrap_args(request_id.0)
    })
    .expect("insert");
    mark_request_running(request_id);
    let status = with_state(|state| state.requests.get(&request_id).map(|v| v.result.status));
    assert_eq!(status, Some(RequestStatus::Running));
}

#[test]
fn apply_unwrap_transfer_result_marks_failure_without_requeue_shape() {
    let mut req = sample_stored_request(RequestStatus::Running);
    super::apply_unwrap_transfer_result(
        &mut req,
        Err("ledger.transfer_failed:temporarily_unavailable".to_string()),
    );
    assert_eq!(req.result.status, RequestStatus::Failed);
    assert_eq!(req.result.ledger_tx_id, None);
    assert_eq!(
        req.result.error_code.as_deref(),
        Some("ledger.transfer_failed:temporarily_unavailable")
    );
}

#[test]
fn apply_unwrap_transfer_result_marks_success_shape() {
    let mut req = sample_stored_request(RequestStatus::Running);
    super::apply_unwrap_transfer_result(&mut req, Ok(vec![0x12, 0x34]));
    assert_eq!(req.result.status, RequestStatus::Succeeded);
    assert_eq!(req.result.ledger_tx_id, Some(vec![0x12, 0x34]));
    assert_eq!(req.result.error_code, None);
}

#[test]
fn reserve_failed_unwrap_retry_requires_recipient_caller() {
    reset_state();
    let request_id = to_request_id(&[0x55u8; 32]).expect("id");
    let recipient = Principal::self_authenticating(b"unwrap-recipient");
    with_state_mut(|state| {
        state
            .requests
            .insert(request_id, sample_failed_unwrap_request_for(recipient));
    });

    let err = super::reserve_failed_unwrap_retry(
        request_id,
        Principal::self_authenticating(b"other-caller"),
    )
    .expect_err("non recipient must fail");
    assert_eq!(err, "unwrap.retry_not_recipient");
}

#[test]
fn reserve_failed_unwrap_retry_marks_running_once() {
    reset_state();
    let request_id = to_request_id(&[0x56u8; 32]).expect("id");
    let recipient = Principal::self_authenticating(b"unwrap-recipient-running");
    with_state_mut(|state| {
        state
            .requests
            .insert(request_id, sample_failed_unwrap_request_for(recipient));
    });

    let reserved = super::reserve_failed_unwrap_retry(request_id, recipient).expect("reserve");
    assert_eq!(reserved.0, vec![2u8; 29]);
    assert_eq!(
        with_state(|state| state.requests.get(&request_id).map(|req| req.result.status)),
        Some(RequestStatus::Running)
    );
    let err =
        super::reserve_failed_unwrap_retry(request_id, recipient).expect_err("second reserve");
    assert_eq!(err, "unwrap.retry_already_running");
}

#[test]
fn reserve_failed_unwrap_retry_remains_available_after_asset_delist() {
    reset_state();
    let request_id = to_request_id(&[0x58u8; 32]).expect("id");
    let recipient = Principal::self_authenticating(b"unwrap-recipient-delisted");
    with_state_mut(|state| {
        state
            .requests
            .insert(request_id, sample_failed_unwrap_request_for(recipient));
        state.allowed_assets.remove(&vec![2u8; 29]);
    });

    let reserved = super::reserve_failed_unwrap_retry(request_id, recipient).expect("reserve");
    assert_eq!(reserved.0, vec![2u8; 29]);
    assert_eq!(
        with_state(|state| state.requests.get(&request_id).map(|req| req.result.status)),
        Some(RequestStatus::Running)
    );
}

#[test]
fn insert_request_accepts_delisted_asset_for_existing_unwrap_liability() {
    reset_state();
    with_state_mut(|state| {
        state.allowed_assets.remove(&vec![2u8; 29]);
    });

    let outcome = insert_request(sample_unwrap_args([0x59u8; 32])).expect("insert");
    assert_eq!(
        outcome,
        InsertRequestOutcome::Inserted(to_request_id(&[0x59u8; 32]).expect("id"))
    );
    with_state(|state| {
        let req = state
            .requests
            .get(&to_request_id(&[0x59u8; 32]).expect("id"))
            .expect("request");
        assert_eq!(req.result.status, RequestStatus::Queued);
        assert_eq!(req.asset_id, vec![2u8; 29]);
    });
}

#[test]
fn reserve_failed_unwrap_retry_rejects_terminal_success() {
    reset_state();
    let request_id = to_request_id(&[0x57u8; 32]).expect("id");
    let recipient = Principal::self_authenticating(b"unwrap-recipient-succeeded");
    let mut req = sample_failed_unwrap_request_for(recipient);
    req.result.status = RequestStatus::Succeeded;
    req.result.ledger_tx_id = Some(vec![0x99]);
    with_state_mut(|state| {
        state.requests.insert(request_id, req);
    });

    let err = super::reserve_failed_unwrap_retry(request_id, recipient).expect_err("succeeded");
    assert_eq!(err, "unwrap.retry_invalid_state");
}

#[test]
fn get_unwrap_requirements_remains_callable_after_asset_delist() {
    reset_state();
    with_state_mut(|state| {
        state.allowed_assets.remove(&vec![2u8; 29]);
    });

    let result = run_ready_future(super::get_unwrap_requirements(GetUnwrapRequirementsArgs {
        asset_id: Principal::from_slice(&[2u8; 29]),
        amount_e8s: Nat::from(1u8),
        caller_evm_address: vec![0x11; 20],
    }));

    match result {
        Err(ApiError::Internal(detail)) => {
            assert_eq!(detail.code, "config.wrap_factory_address_invalid");
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn nat_to_be_bytes_preserves_high_bit_width() {
    let value = Nat(BigUint::from(1u8) << 200usize);
    let encoded = nat_to_be_bytes(&value);
    assert!(encoded.len() > 16);
    assert_eq!(encoded.first().copied(), Some(1u8));
}

#[test]
fn decode_u256_be_accepts_max_uint256() {
    let decoded = decode_u256_be(&[0xffu8; 32]).expect("must decode");
    assert_eq!(decoded, [0xffu8; 32]);
    let nat = Nat(BigUint::from_bytes_be(&decoded));
    assert!(nat > u128::MAX);
}

#[test]
fn transfer_error_to_code_formats_expected_variant() {
    let code = transfer_error_to_code(&Icrc1TransferError::Duplicate {
        duplicate_of: Nat(BigUint::from(42u32)),
    });
    assert_eq!(code, "duplicate:42");
}

#[test]
fn map_transfer_reply_treats_duplicate_as_success() {
    let mapped = map_transfer_reply(Err(Icrc1TransferError::Duplicate {
        duplicate_of: Nat(BigUint::from(42u32)),
    }))
    .expect("duplicate must map to success");
    assert_eq!(mapped, vec![42u8]);
}

#[test]
fn candid_roundtrip_for_icrc1_transfer_result_decodes_nat_and_error() {
    let ok_wire =
        encode_one((Ok::<Nat, Icrc1TransferError>(Nat(BigUint::from(7u32))),)).expect("encode ok");
    let ok_decoded: (Result<Nat, Icrc1TransferError>,) = decode_one(&ok_wire).expect("decode ok");
    let ok_mapped = map_transfer_reply(ok_decoded.0).expect("map ok");
    assert_eq!(ok_mapped, vec![7u8]);

    let err_wire = encode_one((Err::<Nat, Icrc1TransferError>(
        Icrc1TransferError::TemporarilyUnavailable,
    ),))
    .expect("encode err");
    let err_decoded: (Result<Nat, Icrc1TransferError>,) =
        decode_one(&err_wire).expect("decode err");
    let err = map_transfer_reply(err_decoded.0).expect_err("map err");
    assert_eq!(err, "ledger.transfer_failed:temporarily_unavailable");
}

#[test]
fn dequeue_empty_keeps_queue_meta() {
    reset_state();
    let before = with_state(|state| *state.queue_meta.get());
    assert!(dequeue_request().is_none());
    let after = with_state(|state| *state.queue_meta.get());
    assert_eq!(before, QueueMeta::new());
    assert_eq!(after, before);
}

#[test]
fn on_worker_queue_drain_clears_flag_when_queue_empty() {
    reset_state();
    WORKER_SCHEDULED.with(|f| f.set(true));
    on_worker_queue_drain();
    let scheduled = WORKER_SCHEDULED.with(|f| f.get());
    assert!(!scheduled);
}

#[test]
fn schedule_worker_is_idempotent_when_already_scheduled() {
    reset_state();
    WORKER_SCHEDULED.with(|f| f.set(true));
    schedule_worker();
    let scheduled = WORKER_SCHEDULED.with(|f| f.get());
    assert!(scheduled);
}

#[test]
fn wrap_insert_request_rejects_duplicate() {
    reset_state();
    let caller = Principal::self_authenticating(b"wrap-caller-dup");
    let asset_id = vec![2u8; 29];
    let amount = vec![0u8; 32];
    let evm_recipient = vec![4u8; 20];
    let request_id = derive_wrap_request_id(
        caller.as_slice(),
        asset_id.as_slice(),
        amount.as_slice(),
        evm_recipient.as_slice(),
        7,
        200_000,
    );
    let args = NormalizedSubmitWrapRequest {
        request_id: to_request_id(&request_id).expect("id"),
        asset_id,
        amount,
        evm_recipient,
        gas_limit: 200_000,
        max_fee_e8s: 1_000_000,
        quoted_gas_price_wei: 300_000_000_000,
        fee_ledger_canister: test_fee_ledger(),
    };
    let request_id = to_request_id(&request_id).expect("id");
    insert_wrap_request(args.clone(), caller, request_id, test_fee_charge(), 1)
        .expect("first should pass");
    let err = insert_wrap_request(args, caller, request_id, test_fee_charge(), 1)
        .expect_err("second should fail");
    assert_eq!(err, "wrap.request.duplicate");
}

#[test]
fn wrap_request_id_changes_when_evm_nonce_changes() {
    reset_state();
    let caller = Principal::self_authenticating(b"wrap-caller-nonce");
    let asset_id = vec![2u8; 29];
    let amount = vec![3u8; 32];
    let evm_recipient = vec![4u8; 20];

    let first = derive_wrap_request_id(
        caller.as_slice(),
        asset_id.as_slice(),
        amount.as_slice(),
        evm_recipient.as_slice(),
        10,
        200_000,
    );
    let second = derive_wrap_request_id(
        caller.as_slice(),
        asset_id.as_slice(),
        amount.as_slice(),
        evm_recipient.as_slice(),
        11,
        200_000,
    );

    assert_ne!(first, second);
}

#[test]
fn approval_required_only_for_allowance_shortage() {
    assert!(!approval_required_for_readiness(UnwrapReadiness::Ready));
    assert!(!approval_required_for_readiness(
        UnwrapReadiness::TokenNotDeployed
    ));
    assert!(!approval_required_for_readiness(
        UnwrapReadiness::InsufficientBalance
    ));
    assert!(approval_required_for_readiness(
        UnwrapReadiness::InsufficientAllowance
    ));
}

#[test]
fn mark_wrap_request_running_sets_running_status() {
    reset_state();
    let caller = Principal::self_authenticating(b"wrap-caller-running");
    let asset_id = vec![2u8; 29];
    let amount = vec![3u8; 32];
    let evm_recipient = vec![5u8; 20];
    let request_id_raw = derive_wrap_request_id(
        caller.as_slice(),
        asset_id.as_slice(),
        amount.as_slice(),
        evm_recipient.as_slice(),
        9,
        300_000,
    );
    let request_id = to_request_id(&request_id_raw).expect("id");
    insert_wrap_request(
        NormalizedSubmitWrapRequest {
            request_id,
            asset_id,
            amount,
            evm_recipient,
            gas_limit: 300_000,
            max_fee_e8s: 1_000_000,
            quoted_gas_price_wei: 300_000_000_000,
            fee_ledger_canister: test_fee_ledger(),
        },
        caller,
        request_id,
        test_fee_charge(),
        1,
    )
    .expect("insert");
    mark_wrap_request_running(request_id);
    let status = with_state(|state| {
        state
            .wrap_requests
            .get(&request_id)
            .map(|v| (v.result.status, v.result.withdraw_in_progress))
    });
    assert_eq!(status, Some((RequestStatus::Running, false)));
}

#[test]
fn wrap_normalize_submit_rejects_zero_gas_limit() {
    reset_state();
    let err = normalize_submit_wrap_args(SubmitWrapRequestArgs {
        asset_id: Principal::self_authenticating(b"wrap-asset-zero-gas"),
        amount_e8s: Nat::from(3u8),
        evm_recipient: vec![5u8; 20],
        evm_nonce: 0,
        gas_limit: 0,
        max_fee_e8s: Nat::from(1_000_000u64),
        quoted_gas_price_wei: Nat::from(300_000_000_000u64),
        fee_ledger_canister: test_fee_ledger(),
    })
    .expect_err("zero gas limit must fail");
    assert_eq!(err, "arg.gas_limit_invalid");
}

#[test]
fn wrap_quote_approval_accepts_matching_quote() {
    let args = sample_normalized_wrap_request();
    let quote = sample_wrap_quote();
    validate_quote_within_approval(&args, &quote).expect("matching quote");
}

#[test]
fn wrap_quote_approval_rejects_fee_increase_before_transfer() {
    let args = sample_normalized_wrap_request();
    let mut quote = sample_wrap_quote();
    quote.charged_fee_e8s = args.max_fee_e8s + 1;
    let err = validate_quote_within_approval(&args, &quote).expect_err("fee must be bounded");
    assert_eq!(err, "fee.quote_exceeded");
}

#[test]
fn wrap_quote_approval_rejects_gas_price_increase_before_transfer() {
    let args = sample_normalized_wrap_request();
    let mut quote = sample_wrap_quote();
    quote.charged_gas_price_wei = args.quoted_gas_price_wei + 1;
    let err = validate_quote_within_approval(&args, &quote).expect_err("gas price must be bounded");
    assert_eq!(err, "fee.gas_price_exceeded");
}

#[test]
fn wrap_quote_approval_rejects_fee_ledger_change_before_transfer() {
    let args = sample_normalized_wrap_request();
    let mut quote = sample_wrap_quote();
    quote.fee_ledger_canister = Principal::self_authenticating(b"changed-fee-ledger");
    let err = validate_quote_within_approval(&args, &quote).expect_err("ledger must be stable");
    assert_eq!(err, "fee.ledger_changed");
}

#[test]
fn runtime_config_rejects_excessive_cycle_fee() {
    let mut args = sample_init_args(1, [0x22; 20]);
    args.cycle_fee_e8s = super::MAX_CYCLE_FEE_E8S + 1;
    let err = validate_runtime_config(&args).expect_err("cycle fee cap");
    assert_eq!(err, "arg.cycle_fee_e8s_out_of_range");
}

#[test]
fn transfer_from_error_to_code_formats_expected_variant() {
    let code = transfer_from_error_to_code(&Icrc2TransferFromError::InsufficientAllowance {
        allowance: Nat(BigUint::from(9u32)),
    });
    assert_eq!(code, "insufficient_allowance:9");
}

#[test]
fn submit_error_to_code_formats_variant() {
    let code = submit_error_to_code(SubmitTxError::Rejected("nonce_low".to_string()));
    assert_eq!(code, "rejected:nonce_low");
}

#[test]
fn build_submit_ic_tx_args_keeps_charge_as_max_fee_and_splits_priority_fee() {
    let charged_gas_price_wei = 300_000_000_000u128;
    let suggested_priority_fee_wei = 150_000_000_000u128;
    let args = super::build_submit_ic_tx_args(
        vec![0x11; 20],
        7,
        450_000,
        charged_gas_price_wei,
        suggested_priority_fee_wei,
        vec![0xaa, 0xbb],
    );

    assert_eq!(args.to, Some(vec![0x11; 20]));
    assert_eq!(args.from, None);
    assert_eq!(args.gas_limit, 450_000);
    assert_eq!(args.nonce, 7);
    assert_eq!(
        super::nat_to_u128(&args.max_fee_per_gas),
        Some(charged_gas_price_wei)
    );
    assert_eq!(
        super::nat_to_u128(&args.max_priority_fee_per_gas),
        Some(suggested_priority_fee_wei)
    );
    assert_ne!(args.max_fee_per_gas, args.max_priority_fee_per_gas);
    assert_eq!(args.data, vec![0xaa, 0xbb]);
}

#[test]
fn build_submit_ic_tx_args_caps_priority_fee_at_max_fee() {
    let charged_gas_price_wei = 300_000_000_000u128;
    let suggested_priority_fee_wei = 450_000_000_000u128;
    let args = super::build_submit_ic_tx_args(
        vec![0x11; 20],
        7,
        450_000,
        charged_gas_price_wei,
        suggested_priority_fee_wei,
        vec![0xaa, 0xbb],
    );

    assert_eq!(
        super::nat_to_u128(&args.max_fee_per_gas),
        Some(charged_gas_price_wei)
    );
    assert_eq!(
        super::nat_to_u128(&args.max_priority_fee_per_gas),
        Some(charged_gas_price_wei)
    );
}

#[test]
fn compute_total_fee_e8s_keeps_existing_charge_formula() {
    let charged = super::compute_total_fee_e8s(21_000, 300_000_000_000, 1_000_000).expect("fee");
    let expected_gas_fee_e8s = (21_000u128 * 300_000_000_000u128).div_ceil(super::WEI_PER_E8S);
    assert_eq!(charged, expected_gas_fee_e8s + 1_000_000u128);
}

#[test]
fn encode_factory_mint_for_asset_call_data_encodes_selector_and_words() {
    let data =
        encode_factory_mint_for_asset_call_data(&[0x33u8; 29], 8, &[0x11u8; 20], &[0x22u8; 32])
            .expect("encode");
    assert_eq!(data.len(), 196);
    assert_ne!(&data[0..4], &[0u8; 4]);
    assert_eq!(&data[4..36], &u256_from_u64(128));
    assert_eq!(&data[36..68], &u256_from_u64(8));
    assert_eq!(&data[68..80], &[0u8; 12]);
    assert_eq!(&data[80..100], &[0x11u8; 20]);
    assert_eq!(&data[100..132], &[0x22u8; 32]);
    assert_eq!(&data[132..164], &u256_from_u64(29));
    assert_eq!(&data[164..193], &[0x33u8; 29]);
}

#[test]
fn decode_asset_decimals_reads_nat_value() {
    let decimals = decode_asset_decimals(&[
        (
            "icrc1:name".to_string(),
            Icrc1MetadataValue::Text("Token".to_string()),
        ),
        (
            "icrc1:decimals".to_string(),
            Icrc1MetadataValue::Nat(Nat::from(8u8)),
        ),
    ])
    .expect("decimals");
    assert_eq!(decimals, 8);
}

#[test]
fn decode_asset_decimals_rejects_missing_or_invalid_value() {
    let missing = decode_asset_decimals(&[(
        "icrc1:name".to_string(),
        Icrc1MetadataValue::Text("Token".to_string()),
    )])
    .expect_err("missing");
    assert_eq!(missing, "wrap.asset_metadata_failed:decimals_missing");

    let invalid = decode_asset_decimals(&[(
        "icrc1:decimals".to_string(),
        Icrc1MetadataValue::Text("8".to_string()),
    )])
    .expect_err("invalid");
    assert_eq!(invalid, "wrap.asset_decimals_invalid");
}

#[test]
fn wrap_request_result_candid_roundtrip_keeps_withdraw_fields() {
    let value = WrapRequestResult {
        status: RequestStatus::Failed,
        pull_ledger_tx_id: Some(vec![1u8; 4]),
        mint_tx_id: None,
        error_code: Some("mint_failed".to_string()),
        withdrawn: true,
        withdraw_ledger_tx_id: Some(vec![2u8; 4]),
        withdraw_error_code: Some("withdraw.call_failed:oops".to_string()),
        withdraw_in_progress: true,
        mint_failed_recoverable: false,
        fee_ledger_tx_id: Some(vec![3u8; 4]),
        charged_fee_e8s: Some(1_000_000),
        charged_gas_price_wei: Some(300_000_000_000),
    };
    let bytes = encode_one(&value).expect("encode");
    let decoded: WrapRequestResult = decode_one(&bytes).expect("decode");
    assert!(decoded.withdrawn);
    assert_eq!(decoded.withdraw_ledger_tx_id, Some(vec![2u8; 4]));
    assert_eq!(
        decoded.withdraw_error_code.as_deref(),
        Some("withdraw.call_failed:oops")
    );
    assert!(decoded.withdraw_in_progress);
}

#[test]
fn mint_failed_recoverable_is_set_on_mint_failure_outcome_shape() {
    let req = WrapStoredRequest {
        caller: candid::Principal::self_authenticating(b"wrap-test-caller")
            .as_slice()
            .to_vec(),
        asset_id: vec![7u8; 29],
        amount: vec![0u8; 32],
        evm_recipient: vec![9u8; 20],
        gas_limit: 300_000,
        fee_created_at_time: 1,
        pull_created_at_time: 1,
        withdraw_created_at_time: 0,
        result: WrapRequestResult {
            status: RequestStatus::Failed,
            pull_ledger_tx_id: Some(vec![1u8; 4]),
            mint_tx_id: None,
            error_code: Some("evm_gateway.submit_failed:rejected:nonce".to_string()),
            withdrawn: false,
            withdraw_ledger_tx_id: None,
            withdraw_error_code: None,
            withdraw_in_progress: false,
            mint_failed_recoverable: true,
            fee_ledger_tx_id: Some(vec![3u8; 4]),
            charged_fee_e8s: Some(1_000_000),
            charged_gas_price_wei: Some(300_000_000_000),
        },
    };
    assert!(req.result.mint_failed_recoverable);
    assert!(req.result.pull_ledger_tx_id.is_some());
}

#[test]
fn validate_withdraw_request_checks_owner_and_state() {
    let owner = candid::Principal::self_authenticating(b"wrap-owner");
    let other = candid::Principal::self_authenticating(b"wrap-other");
    let base = WrapStoredRequest {
        caller: owner.as_slice().to_vec(),
        asset_id: vec![7u8; 29],
        amount: vec![0u8; 32],
        evm_recipient: vec![9u8; 20],
        gas_limit: 300_000,
        fee_created_at_time: 1,
        pull_created_at_time: 1,
        withdraw_created_at_time: 0,
        result: WrapRequestResult {
            status: RequestStatus::Failed,
            pull_ledger_tx_id: Some(vec![1u8; 4]),
            mint_tx_id: None,
            error_code: Some("mint_failed".to_string()),
            withdrawn: false,
            withdraw_ledger_tx_id: None,
            withdraw_error_code: None,
            withdraw_in_progress: false,
            mint_failed_recoverable: true,
            fee_ledger_tx_id: Some(vec![3u8; 4]),
            charged_fee_e8s: Some(1_000_000),
            charged_gas_price_wei: Some(300_000_000_000),
        },
    };
    validate_withdraw_request(&base, owner).expect("eligible");
    let not_owner = validate_withdraw_request(&base, other).expect_err("owner check");
    assert_eq!(not_owner, "withdraw.not_request_owner");

    let mut non_recoverable = base.clone();
    non_recoverable.result.mint_failed_recoverable = false;
    let invalid = validate_withdraw_request(&non_recoverable, owner).expect_err("state");
    assert_eq!(invalid, "withdraw.invalid_state");

    let mut withdrawn = base.clone();
    withdrawn.result.withdrawn = true;
    let already = validate_withdraw_request(&withdrawn, owner).expect_err("withdrawn");
    assert_eq!(already, "withdraw.already_withdrawn");

    let mut in_progress = base;
    in_progress.result.withdraw_in_progress = true;
    let blocked = validate_withdraw_request(&in_progress, owner).expect_err("in progress");
    assert_eq!(blocked, "withdraw.in_progress");
}

#[test]
fn reserve_failed_wrap_withdraw_remains_available_after_asset_delist() {
    reset_state();
    let request_id = to_request_id(&[0x66u8; 32]).expect("id");
    let owner = Principal::self_authenticating(b"wrap-owner-delisted");
    with_state_mut(|state| {
        state.wrap_requests.insert(
            request_id,
            WrapStoredRequest {
                caller: owner.as_slice().to_vec(),
                asset_id: vec![7u8; 29],
                amount: vec![0u8; 32],
                evm_recipient: vec![9u8; 20],
                gas_limit: 300_000,
                fee_created_at_time: 1,
                pull_created_at_time: 1,
                withdraw_created_at_time: 0,
                result: WrapRequestResult {
                    status: RequestStatus::Failed,
                    pull_ledger_tx_id: Some(vec![1u8; 4]),
                    mint_tx_id: None,
                    error_code: Some("mint_failed".to_string()),
                    withdrawn: false,
                    withdraw_ledger_tx_id: None,
                    withdraw_error_code: None,
                    withdraw_in_progress: false,
                    mint_failed_recoverable: true,
                    fee_ledger_tx_id: Some(vec![3u8; 4]),
                    charged_fee_e8s: Some(1_000_000),
                    charged_gas_price_wei: Some(300_000_000_000),
                },
            },
        );
        state.allowed_assets.remove(&vec![7u8; 29]);
    });

    let reserved = super::reserve_failed_wrap_withdraw(request_id, owner).expect("reserve");
    assert_eq!(reserved.0, vec![7u8; 29]);
    assert_eq!(
        with_state(|state| {
            state
                .wrap_requests
                .get(&request_id)
                .map(|req| req.result.withdraw_in_progress)
        }),
        Some(true)
    );
}

#[test]
fn wrap_worker_keeps_processing_delisted_asset_after_acceptance() {
    reset_state();
    let request_id = to_request_id(&[0x67u8; 32]).expect("id");
    with_state_mut(|state| {
        state.wrap_requests.insert(
            request_id,
            WrapStoredRequest {
                // invalid principal bytes で transfer_from の入口まで進んだことを確認する。
                // allowlist 再チェックが残っていると asset.not_allowed でここまで到達しない。
                caller: Vec::new(),
                asset_id: vec![7u8; 29],
                amount: vec![8u8; 32],
                evm_recipient: vec![9u8; 20],
                gas_limit: 300_000,
                fee_created_at_time: 1,
                pull_created_at_time: 1,
                withdraw_created_at_time: 0,
                result: WrapRequestResult {
                    status: RequestStatus::Queued,
                    pull_ledger_tx_id: None,
                    mint_tx_id: None,
                    error_code: None,
                    withdrawn: false,
                    withdraw_ledger_tx_id: None,
                    withdraw_error_code: None,
                    withdraw_in_progress: false,
                    mint_failed_recoverable: false,
                    fee_ledger_tx_id: Some(vec![3u8; 4]),
                    charged_fee_e8s: Some(1_000_000),
                    charged_gas_price_wei: Some(300_000_000_000),
                },
            },
        );
        let mut meta = *state.wrap_queue_meta.get();
        let seq = meta.tail;
        meta.tail = meta.tail.saturating_add(1);
        state.wrap_queue.insert(seq, request_id);
        let _ = state.wrap_queue_meta.set(meta);
        state.allowed_assets.remove(&vec![7u8; 29]);
    });

    run_ready_future(super::wrap_worker_tick());

    with_state(|state| {
        let req = state.wrap_requests.get(&request_id).expect("request");
        assert_eq!(req.result.status, RequestStatus::Failed);
        assert_eq!(req.result.pull_ledger_tx_id, None);
        assert_eq!(
            req.result.error_code.as_deref(),
            Some("arg.principal_invalid")
        );
        assert_ne!(req.result.error_code.as_deref(), Some("asset.not_allowed"));
        assert!(!req.result.mint_failed_recoverable);
    });
}

#[test]
fn is_withdrawable_matches_expected_shape() {
    let owner = Principal::self_authenticating(b"wrap-owner");
    let req = WrapStoredRequest {
        caller: owner.as_slice().to_vec(),
        asset_id: vec![7u8; 29],
        amount: vec![0u8; 32],
        evm_recipient: vec![9u8; 20],
        gas_limit: 300_000,
        fee_created_at_time: 1,
        pull_created_at_time: 1,
        withdraw_created_at_time: 0,
        result: WrapRequestResult {
            status: RequestStatus::Failed,
            pull_ledger_tx_id: Some(vec![1u8; 4]),
            mint_tx_id: None,
            error_code: Some("mint_failed".to_string()),
            withdrawn: false,
            withdraw_ledger_tx_id: None,
            withdraw_error_code: None,
            withdraw_in_progress: false,
            mint_failed_recoverable: true,
            fee_ledger_tx_id: Some(vec![3u8; 4]),
            charged_fee_e8s: Some(1_000_000),
            charged_gas_price_wei: Some(300_000_000_000),
        },
    };
    assert!(is_withdrawable(&req));
}

#[test]
fn withdraw_error_code_mapping_is_stable() {
    assert_eq!(
        to_withdraw_error_code("ledger.transfer_failed:insufficient_funds:1"),
        "withdraw.transfer_failed:insufficient_funds:1"
    );
    assert_eq!(
        to_withdraw_error_code("ledger.decode_failed:bad wire"),
        "withdraw.decode_failed:bad wire"
    );
    assert_eq!(
        to_withdraw_error_code("ledger.call_failed:canister reject"),
        "withdraw.call_failed:canister reject"
    );
}

#[test]
fn on_wrap_worker_queue_drain_clears_flag_when_queue_empty() {
    reset_state();
    WRAP_WORKER_SCHEDULED.with(|f| f.set(true));
    on_wrap_worker_queue_drain();
    let scheduled = WRAP_WORKER_SCHEDULED.with(|f| f.get());
    assert!(!scheduled);
}

#[test]
fn schedule_wrap_worker_is_idempotent_when_already_scheduled() {
    reset_state();
    WRAP_WORKER_SCHEDULED.with(|f| f.set(true));
    schedule_wrap_worker();
    let scheduled = WRAP_WORKER_SCHEDULED.with(|f| f.get());
    assert!(scheduled);
}

#[test]
fn recover_wrap_request_state_after_upgrade_requeues_running_request() {
    reset_state();
    let request_id = to_request_id(&[0x41u8; 32]).expect("id");
    with_state_mut(|state| {
        state.wrap_requests.insert(
            request_id,
            WrapStoredRequest {
                caller: Principal::self_authenticating(b"wrap-running")
                    .as_slice()
                    .to_vec(),
                asset_id: vec![7u8; 29],
                amount: vec![8u8; 32],
                evm_recipient: vec![9u8; 20],
                gas_limit: 300_000,
                fee_created_at_time: 1,
                pull_created_at_time: 1,
                withdraw_created_at_time: 0,
                result: WrapRequestResult {
                    status: RequestStatus::Running,
                    pull_ledger_tx_id: None,
                    mint_tx_id: None,
                    error_code: None,
                    withdrawn: false,
                    withdraw_ledger_tx_id: None,
                    withdraw_error_code: None,
                    withdraw_in_progress: false,
                    mint_failed_recoverable: false,
                    fee_ledger_tx_id: Some(vec![3u8; 4]),
                    charged_fee_e8s: Some(1_000_000),
                    charged_gas_price_wei: Some(300_000_000_000),
                },
            },
        );
    });

    assert!(recover_wrap_request_state_after_upgrade(123));
    with_state(|state| {
        let req = state.wrap_requests.get(&request_id).expect("request");
        assert_eq!(req.result.status, RequestStatus::Queued);
        assert_eq!(req.result.fee_ledger_tx_id, Some(vec![3u8; 4]));
        assert_eq!(req.result.charged_fee_e8s, Some(1_000_000));
        assert_eq!(req.result.charged_gas_price_wei, Some(300_000_000_000));
        assert_eq!(state.wrap_queue.len(), 1);
        assert_eq!(state.wrap_queue_meta.get().tail, 1);
        assert_eq!(state.wrap_queue.get(&0), Some(request_id));
    });
}

#[test]
fn recover_wrap_request_state_after_upgrade_does_not_duplicate_existing_queue_entry() {
    reset_state();
    let request_id = to_request_id(&[0x42u8; 32]).expect("id");
    with_state_mut(|state| {
        state.wrap_requests.insert(
            request_id,
            WrapStoredRequest {
                caller: Principal::self_authenticating(b"wrap-queued")
                    .as_slice()
                    .to_vec(),
                asset_id: vec![7u8; 29],
                amount: vec![8u8; 32],
                evm_recipient: vec![9u8; 20],
                gas_limit: 300_000,
                fee_created_at_time: 1,
                pull_created_at_time: 1,
                withdraw_created_at_time: 0,
                result: WrapRequestResult {
                    status: RequestStatus::Queued,
                    pull_ledger_tx_id: None,
                    mint_tx_id: None,
                    error_code: None,
                    withdrawn: false,
                    withdraw_ledger_tx_id: None,
                    withdraw_error_code: None,
                    withdraw_in_progress: false,
                    mint_failed_recoverable: false,
                    fee_ledger_tx_id: Some(vec![3u8; 4]),
                    charged_fee_e8s: Some(1_000_000),
                    charged_gas_price_wei: Some(300_000_000_000),
                },
            },
        );
        let mut meta = *state.wrap_queue_meta.get();
        let seq = meta.tail;
        meta.tail = meta.tail.saturating_add(1);
        state.wrap_queue.insert(seq, request_id);
        state.wrap_queue_meta.set(meta);
    });

    assert!(recover_wrap_request_state_after_upgrade(123));
    with_state(|state| {
        assert_eq!(state.wrap_queue.len(), 1);
        assert_eq!(state.wrap_queue_meta.get().tail, 1);
        assert_eq!(state.wrap_queue.get(&0), Some(request_id));
    });
}

#[test]
fn recover_wrap_request_state_after_upgrade_keeps_terminal_requests_out_of_queue() {
    reset_state();
    let request_id = to_request_id(&[0x43u8; 32]).expect("id");
    with_state_mut(|state| {
        state.wrap_requests.insert(
            request_id,
            WrapStoredRequest {
                caller: Principal::self_authenticating(b"wrap-failed")
                    .as_slice()
                    .to_vec(),
                asset_id: vec![7u8; 29],
                amount: vec![8u8; 32],
                evm_recipient: vec![9u8; 20],
                gas_limit: 300_000,
                fee_created_at_time: 1,
                pull_created_at_time: 1,
                withdraw_created_at_time: 0,
                result: WrapRequestResult {
                    status: RequestStatus::Failed,
                    pull_ledger_tx_id: Some(vec![1u8; 4]),
                    mint_tx_id: None,
                    error_code: Some("evm_gateway.submit_failed:rejected:nonce".to_string()),
                    withdrawn: false,
                    withdraw_ledger_tx_id: None,
                    withdraw_error_code: None,
                    withdraw_in_progress: false,
                    mint_failed_recoverable: true,
                    fee_ledger_tx_id: Some(vec![3u8; 4]),
                    charged_fee_e8s: Some(1_000_000),
                    charged_gas_price_wei: Some(300_000_000_000),
                },
            },
        );
    });

    assert!(!recover_wrap_request_state_after_upgrade(123));
    with_state(|state| {
        let req = state.wrap_requests.get(&request_id).expect("request");
        assert_eq!(req.result.status, RequestStatus::Failed);
        assert_eq!(req.result.pull_ledger_tx_id, Some(vec![1u8; 4]));
        assert!(req.result.mint_failed_recoverable);
        assert_eq!(state.wrap_queue.len(), 0);
    });
}

#[test]
fn recover_wrap_request_state_after_upgrade_clears_withdraw_in_progress() {
    reset_state();
    let request_id = to_request_id(&[0x44u8; 32]).expect("id");
    with_state_mut(|state| {
        state.wrap_requests.insert(
            request_id,
            WrapStoredRequest {
                caller: Principal::self_authenticating(b"wrap-withdraw-in-progress")
                    .as_slice()
                    .to_vec(),
                asset_id: vec![7u8; 29],
                amount: vec![8u8; 32],
                evm_recipient: vec![9u8; 20],
                gas_limit: 300_000,
                fee_created_at_time: 1,
                pull_created_at_time: 1,
                withdraw_created_at_time: 1,
                result: WrapRequestResult {
                    status: RequestStatus::Failed,
                    pull_ledger_tx_id: Some(vec![1u8; 4]),
                    mint_tx_id: None,
                    error_code: Some("recover_failed".to_string()),
                    withdrawn: false,
                    withdraw_ledger_tx_id: None,
                    withdraw_error_code: None,
                    withdraw_in_progress: true,
                    mint_failed_recoverable: true,
                    fee_ledger_tx_id: Some(vec![3u8; 4]),
                    charged_fee_e8s: Some(1_000_000),
                    charged_gas_price_wei: Some(300_000_000_000),
                },
            },
        );
    });

    assert!(!recover_wrap_request_state_after_upgrade(123));
    with_state(|state| {
        let req = state.wrap_requests.get(&request_id).expect("request");
        assert!(!req.result.withdraw_in_progress);
        assert_eq!(req.result.status, RequestStatus::Failed);
        assert_eq!(state.wrap_queue.len(), 0);
    });
}

#[test]
fn native_withdraw_receive_amount_rejects_fee_or_less() {
    assert_eq!(native_withdraw_receive_amount(10_001, 10_000), Ok(1));
    assert_eq!(
        native_withdraw_receive_amount(10_000, 10_000),
        Err("native_withdraw.amount_not_above_fee")
    );
    assert_eq!(
        native_withdraw_receive_amount(9_999, 10_000),
        Err("native_withdraw.amount_not_above_fee")
    );
}
