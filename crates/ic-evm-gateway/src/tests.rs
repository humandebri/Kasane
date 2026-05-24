use super::{
    clamp_return_data, decode_precompile_allow_key_for_principal, inspect_lightweight_tx_guard,
    inspect_payload_limit_for_method, inspect_policy_for_method, migration_pending,
    parse_submit_ic_tx_args, pop_next_dispatch_request, pop_next_icp_update_request,
    precompile_allow_key_for_principal, reject_anonymous_principal, reject_write_reason,
    should_run_cycle_observer_migration_tick, should_schedule_mining_after_cycle_observer,
    tx_id_from_bytes, validate_prune_policy_input, validate_query_precompile_allow_args,
    validate_update_precompile_allow_args, ApiError, EthLogFilterView, ExecuteTxError,
    GenesisBalanceView, GetLogsErrorView, InitArgs, PrecompileAllowArgs, PrunePolicyView,
    QuoteNativeDepositArgs, QuoteWrapRequestArgs, SubmitIcTxArgsDto, WrapConfigArgs,
    DEFAULT_BLOCK_GAS_LIMIT, DEFAULT_MIN_FEE_FLOOR, INSPECT_METHOD_POLICIES, MINING_ERROR_COUNT,
    PRUNE_ERROR_COUNT,
};
use candid::{encode_one, Nat, Principal};
use evm_core::chain;
use evm_core::chain::{ChainError, ExecResult, TxIn};
use evm_core::hash;
use evm_core::kasane_precompiles::{
    ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS, NATIVE_WITHDRAW_PRECOMPILE_ADDRESS,
    WRAP_PRECOMPILE_ADDRESS,
};
use evm_core::revm_exec::{ExecError, OpHaltReason, OpTransactionError};
use evm_core::tx_decode::{encode_ic_synthetic_input, IcSyntheticTxInput};
use evm_db::chain_data::constants::MAX_RETURN_DATA;
use evm_db::chain_data::receipt::log_entry_from_parts;
use evm_db::chain_data::{
    BlockData, IcpUpdateDispatchRequest, IcpUpdateRequestStatus, MigrationPhase, MintSubmitStatus,
    OpsMode, ReceiptLike, RequestStatus, RuntimeConfigV1, StoredTxBytes, TxId, TxIndexEntry,
    TxKind, TxLoc, TxLocKind, UnwrapDispatchRequest, UnwrapRequestStatus, WrapPendingSubmission,
    WrapRequestResult, WrapRequestStage, WrapStoredRequest, WRAP_DECODE_FAILURE_CODE,
};
use evm_db::memory::{get_memory, AppMemoryId, WASM_PAGE_SIZE_BYTES};
use evm_db::meta::{
    current_schema_version, schema_migration_state, set_meta, set_needs_migration,
    set_schema_migration_state, SchemaMigrationPhase, SchemaMigrationState,
};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};
use evm_db::types::keys::{make_account_key, make_storage_key};
use evm_db::types::values::{AccountVal, U256Val};
use evm_db::{Memory, Storable};
use ic_cdk::call::{CallFailed, CallPerformFailed, CallRejected, RejectCode};
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::future::Future;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::pin;
use std::str::FromStr;
use std::task::{Context, Poll, Waker};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TxApiErrorKind {
    InvalidArgument,
    Rejected,
}

const CODE_ARG_TX_TOO_LARGE: &str = "arg.tx_too_large";
const CODE_ARG_DECODE_FAILED: &str = "arg.decode_failed";
const CODE_ARG_DERIVATION_FAILED: &str = "arg.principal_to_evm_derivation_failed";
const CODE_ARG_UNSUPPORTED_TX_KIND: &str = "arg.unsupported_tx_kind";
const CODE_SUBMIT_TX_ALREADY_SEEN: &str = "submit.tx_already_seen";
const CODE_SUBMIT_INVALID_FEE: &str = "submit.invalid_fee";
const CODE_SUBMIT_NONCE_TOO_LOW: &str = "submit.nonce_too_low";
const CODE_SUBMIT_NONCE_GAP: &str = "submit.nonce_gap";
const CODE_SUBMIT_NONCE_CONFLICT: &str = "submit.nonce_conflict";
const CODE_SUBMIT_QUEUE_FULL: &str = "submit.queue_full";
const CODE_SUBMIT_SENDER_QUEUE_FULL: &str = "submit.sender_queue_full";
const CODE_SUBMIT_PRINCIPAL_QUEUE_FULL: &str = "submit.principal_queue_full";
const CODE_SUBMIT_DECODE_RATE_LIMITED: &str = "submit.decode_rate_limited";
const CODE_INTERNAL_UNEXPECTED: &str = "internal.unexpected";
const POST_UPGRADE_MIGRATION_MAX_ITERS: usize = 32;
const POST_UPGRADE_SCHEMA_MIGRATION_STEPS: u32 = 1024;
const POST_UPGRADE_STATE_ROOT_MIGRATION_STEPS: u32 = 1024;

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

fn run_post_upgrade_migrations_until_settled() {
    for _ in 0..POST_UPGRADE_MIGRATION_MAX_ITERS {
        if !migration_pending() {
            return;
        }
        super::drive_migrations_tick(
            POST_UPGRADE_SCHEMA_MIGRATION_STEPS,
            POST_UPGRADE_STATE_ROOT_MIGRATION_STEPS,
        );
    }
    panic!(
        "post-upgrade schema/state-root migrations did not settle within {} iterations",
        POST_UPGRADE_MIGRATION_MAX_ITERS
    );
}

fn install_runtime_wrap_canister_id(wrap_canister_id: Principal) {
    evm_db::stable_state::set_runtime_config(
        RuntimeConfigV1::try_new_from_bytes(wrap_canister_id.as_slice(), [0x44u8; 20])
            .expect("runtime config"),
    );
}

fn decode_failure_label_view(raw: [u8; 32]) -> Option<String> {
    ic_evm_ops::decode_failure_label_view(raw)
}

fn exec_error_to_code(err: Option<&evm_core::revm_exec::ExecError>) -> &'static str {
    use evm_core::revm_exec::{ExecError, OpHaltReason, OpTransactionError};

    match err {
        None => "exec.execution.failed",
        Some(ExecError::Decode(_)) => "exec.decode.failed",
        Some(ExecError::TxError(OpTransactionError::TxBuildFailed)) => "exec.tx.build_failed",
        Some(ExecError::TxError(OpTransactionError::TxRejectedByPolicy)) => {
            "exec.tx.rejected_by_policy"
        }
        Some(ExecError::TxError(OpTransactionError::TxPrecheckFailed)) => "exec.tx.precheck_failed",
        Some(ExecError::TxError(OpTransactionError::TxExecutionFailed)) => {
            "exec.tx.execution_failed"
        }
        Some(ExecError::Revert) => "exec.revert",
        Some(ExecError::EvmHalt(OpHaltReason::OutOfGas)) => "exec.halt.out_of_gas",
        Some(ExecError::EvmHalt(OpHaltReason::InvalidOpcode)) => "exec.halt.invalid_opcode",
        Some(ExecError::EvmHalt(OpHaltReason::StackOverflow)) => "exec.halt.stack_overflow",
        Some(ExecError::EvmHalt(OpHaltReason::StackUnderflow)) => "exec.halt.stack_underflow",
        Some(ExecError::EvmHalt(OpHaltReason::InvalidJump)) => "exec.halt.invalid_jump",
        Some(ExecError::EvmHalt(OpHaltReason::StateChangeDuringStaticCall)) => {
            "exec.halt.static_state_change"
        }
        Some(ExecError::EvmHalt(OpHaltReason::PrecompileError)) => "exec.halt.precompile_error",
        Some(ExecError::EvmHalt(OpHaltReason::Unknown)) => "exec.halt.unknown",
        Some(ExecError::InvalidGasFee) => "exec.gas_fee.invalid",
        Some(ExecError::ResultTooLarge) => "exec.result.too_large",
        Some(ExecError::InstructionBudgetExceeded) => "exec.budget.instruction_exceeded",
        Some(ExecError::ExternalQuery(_)) => "exec.external_query",
        Some(ExecError::ExecutionFailed) => "exec.execution.failed",
    }
}

fn submit_reject_code(err: &ChainError) -> Option<&'static str> {
    match err {
        ChainError::TxAlreadySeen => Some(CODE_SUBMIT_TX_ALREADY_SEEN),
        ChainError::InvalidFee => Some(CODE_SUBMIT_INVALID_FEE),
        ChainError::NonceTooLow => Some(CODE_SUBMIT_NONCE_TOO_LOW),
        ChainError::NonceGap => Some(CODE_SUBMIT_NONCE_GAP),
        ChainError::NonceConflict => Some(CODE_SUBMIT_NONCE_CONFLICT),
        ChainError::QueueFull => Some(CODE_SUBMIT_QUEUE_FULL),
        ChainError::SenderQueueFull => Some(CODE_SUBMIT_SENDER_QUEUE_FULL),
        ChainError::PrincipalQueueFull => Some(CODE_SUBMIT_PRINCIPAL_QUEUE_FULL),
        ChainError::DecodeRateLimited => Some(CODE_SUBMIT_DECODE_RATE_LIMITED),
        _ => None,
    }
}

fn encode_unwrap_payload(asset: Principal, amount: [u8; 32], recipient: Principal) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + 30 + 32 + 30);
    out.push(1);
    out.push(asset.as_slice().len() as u8);
    out.extend_from_slice(asset.as_slice());
    out.resize(1 + 1 + 29, 0);
    out.extend_from_slice(&amount);
    out.push(recipient.as_slice().len() as u8);
    out.extend_from_slice(recipient.as_slice());
    out.resize(1 + 1 + 29 + 32 + 1 + 29, 0);
    out
}

#[test]
fn icrc10_supported_standards_advertise_icrc21() {
    let standards = super::icrc21::supported_standards();
    assert!(standards.iter().any(|item| item.name == "ICRC-21"));
}

#[test]
fn query_precompile_allow_args_follow_verified_model() {
    let valid = PrecompileAllowArgs {
        target: Principal::self_authenticating(b"query-allow-target"),
        method: "read_state".to_string(),
    };
    assert!(validate_query_precompile_allow_args(&valid).is_ok());

    let anonymous = PrecompileAllowArgs {
        target: Principal::anonymous(),
        method: "read_state".to_string(),
    };
    assert_eq!(
        validate_query_precompile_allow_args(&anonymous).unwrap_err(),
        "arg.target_anonymous"
    );

    let empty_method = PrecompileAllowArgs {
        target: valid.target,
        method: String::new(),
    };
    assert_eq!(
        validate_query_precompile_allow_args(&empty_method).unwrap_err(),
        "arg.method_invalid"
    );

    let long_method = PrecompileAllowArgs {
        target: valid.target,
        method: "x".repeat(65),
    };
    assert_eq!(
        validate_query_precompile_allow_args(&long_method).unwrap_err(),
        "arg.method_invalid"
    );

    let non_ascii_method = PrecompileAllowArgs {
        target: valid.target,
        method: "状態".to_string(),
    };
    assert_eq!(
        validate_query_precompile_allow_args(&non_ascii_method).unwrap_err(),
        "arg.method_invalid"
    );
}

#[test]
fn update_precompile_allow_args_follow_verified_model() {
    let valid = PrecompileAllowArgs {
        target: Principal::self_authenticating(b"update-allow-target"),
        method: "write_state".to_string(),
    };
    assert!(validate_update_precompile_allow_args(&valid).is_ok());

    let anonymous = PrecompileAllowArgs {
        target: Principal::anonymous(),
        method: "write_state".to_string(),
    };
    assert_eq!(
        validate_update_precompile_allow_args(&anonymous).unwrap_err(),
        "arg.target_anonymous"
    );

    let non_ascii_method = PrecompileAllowArgs {
        target: valid.target,
        method: "更新".to_string(),
    };
    assert_eq!(
        validate_update_precompile_allow_args(&non_ascii_method).unwrap_err(),
        "arg.method_invalid"
    );
}

#[test]
fn precompile_allow_key_for_principal_decodes_valid_boundary() {
    let target = Principal::self_authenticating(b"query-allow-target");
    let key = precompile_allow_key_for_principal(target, "read_state");
    let decoded = decode_precompile_allow_key_for_principal(&key).expect("decode allowlist key");

    assert_eq!(decoded.target, target);
    assert_eq!(decoded.method, "read_state");
}

#[test]
fn icrc21_submit_ic_tx_consent_decodes_precompile_unwrap() {
    let asset = Principal::self_authenticating(b"wrap-asset");
    let recipient = Principal::self_authenticating(b"wrap-recipient");
    let mut amount = [0u8; 32];
    amount[31] = 42;
    let response = run_ready_future(super::icrc21::consent_message(
        super::icrc21::Icrc21ConsentMessageRequest {
            method: "submit_ic_tx".to_string(),
            arg: encode_one(SubmitIcTxArgsDto {
                to: Some(WRAP_PRECOMPILE_ADDRESS.to_vec()),
                value: Nat::from(0u8),
                max_priority_fee_per_gas: Nat::from(2u8),
                data: encode_unwrap_payload(asset, amount, recipient),
                from: None,
                max_fee_per_gas: Nat::from(3u8),
                nonce: 9,
                gas_limit: 210_000,
            })
            .expect("encode submit_ic_tx"),
            user_preferences: super::icrc21::Icrc21ConsentMessageSpec {
                metadata: super::icrc21::Icrc21ConsentMessageMetadata {
                    utc_offset_minutes: Some(540),
                    language: "en".to_string(),
                },
                device_spec: None,
            },
        },
    ))
    .expect("consent ok");
    let super::icrc21::Icrc21ConsentMessage::GenericDisplayMessage(markdown) =
        response.consent_message
    else {
        panic!("expected generic display");
    };
    assert!(markdown.contains("Approve Kasane unwrap"));
    assert!(markdown.contains(&asset.to_text()));
    assert!(markdown.contains(&recipient.to_text()));
    assert!(markdown.contains("amount_e8s: `42`"));
}

#[test]
fn icrc21_submit_ic_tx_consent_decodes_erc20_approve() {
    let mut data = vec![0x09, 0x5e, 0xa7, 0xb3];
    data.extend_from_slice(&[0u8; 12]);
    data.extend_from_slice(&[0x44; 20]);
    data.extend_from_slice(&[0u8; 31]);
    data.push(0x2a);
    let response = run_ready_future(super::icrc21::consent_message(
        super::icrc21::Icrc21ConsentMessageRequest {
            method: "submit_ic_tx".to_string(),
            arg: encode_one(SubmitIcTxArgsDto {
                to: Some(vec![0x22; 20]),
                value: Nat::from(0u8),
                max_priority_fee_per_gas: Nat::from(2u8),
                data,
                from: None,
                max_fee_per_gas: Nat::from(3u8),
                nonce: 11,
                gas_limit: 90_000,
            })
            .expect("encode submit_ic_tx"),
            user_preferences: super::icrc21::Icrc21ConsentMessageSpec {
                metadata: super::icrc21::Icrc21ConsentMessageMetadata {
                    utc_offset_minutes: Some(540),
                    language: "en".to_string(),
                },
                device_spec: None,
            },
        },
    ))
    .expect("consent ok");
    let super::icrc21::Icrc21ConsentMessage::GenericDisplayMessage(markdown) =
        response.consent_message
    else {
        panic!("expected generic display");
    };
    assert!(markdown.contains("Approve ERC-20 allowance transaction"));
    assert!(markdown.contains("0x4444444444444444444444444444444444444444"));
    assert!(markdown.contains("amount: `42`"));
}

#[test]
fn icrc21_submit_ic_tx_consent_falls_back_to_generic_message() {
    let response = run_ready_future(super::icrc21::consent_message(
        super::icrc21::Icrc21ConsentMessageRequest {
            method: "submit_ic_tx".to_string(),
            arg: encode_one(SubmitIcTxArgsDto {
                to: Some(vec![0x33; 20]),
                value: Nat::from(7u8),
                max_priority_fee_per_gas: Nat::from(2u8),
                data: vec![0xaa, 0xbb, 0xcc],
                from: None,
                max_fee_per_gas: Nat::from(3u8),
                nonce: 13,
                gas_limit: 120_000,
            })
            .expect("encode submit_ic_tx"),
            user_preferences: super::icrc21::Icrc21ConsentMessageSpec {
                metadata: super::icrc21::Icrc21ConsentMessageMetadata {
                    utc_offset_minutes: Some(540),
                    language: "en".to_string(),
                },
                device_spec: None,
            },
        },
    ))
    .expect("consent ok");
    let super::icrc21::Icrc21ConsentMessage::GenericDisplayMessage(markdown) =
        response.consent_message
    else {
        panic!("expected generic display");
    };
    assert!(markdown.contains("Approve Kasane transaction"));
    assert!(markdown.contains("data size: `3` bytes"));
}

#[test]
fn icrc21_submit_ic_tx_consent_accepts_line_display_request() {
    let response = run_ready_future(super::icrc21::consent_message(
        super::icrc21::Icrc21ConsentMessageRequest {
            method: "submit_ic_tx".to_string(),
            arg: encode_one(SubmitIcTxArgsDto {
                to: Some(vec![0x33; 20]),
                value: Nat::from(7u8),
                max_priority_fee_per_gas: Nat::from(2u8),
                data: vec![0xaa, 0xbb, 0xcc],
                from: None,
                max_fee_per_gas: Nat::from(3u8),
                nonce: 13,
                gas_limit: 120_000,
            })
            .expect("encode submit_ic_tx"),
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
    ))
    .expect("consent ok");
    assert!(matches!(
        response.consent_message,
        super::icrc21::Icrc21ConsentMessage::GenericDisplayMessage(_)
    ));
}

#[test]
fn evm_did_does_not_export_fields_display_icrc21_shape() {
    let did = include_str!("../evm_canister.did");
    assert!(!did.contains("FieldsDisplay"));
    assert!(did.contains("LineDisplay"));
}

fn chain_submit_error_to_code(err: &ChainError) -> Option<(TxApiErrorKind, &'static str)> {
    match err {
        ChainError::TxTooLarge => Some((TxApiErrorKind::InvalidArgument, CODE_ARG_TX_TOO_LARGE)),
        ChainError::DecodeFailed => Some((TxApiErrorKind::InvalidArgument, CODE_ARG_DECODE_FAILED)),
        ChainError::AddressDerivationFailed => {
            Some((TxApiErrorKind::InvalidArgument, CODE_ARG_DERIVATION_FAILED))
        }
        ChainError::UnsupportedTxKind => Some((
            TxApiErrorKind::InvalidArgument,
            CODE_ARG_UNSUPPORTED_TX_KIND,
        )),
        _ => submit_reject_code(err).map(|code| (TxApiErrorKind::Rejected, code)),
    }
}

fn map_submit_chain_error(err: ChainError, op_name: &str) -> super::SubmitTxError {
    if let Some((kind, code)) = chain_submit_error_to_code(&err) {
        return match kind {
            TxApiErrorKind::InvalidArgument => {
                super::SubmitTxError::InvalidArgument(code.to_string())
            }
            TxApiErrorKind::Rejected => super::SubmitTxError::Rejected(code.to_string()),
        };
    }
    tracing::error!(error = ?err, operation = op_name, "submit transaction failed");
    super::SubmitTxError::Internal(CODE_INTERNAL_UNEXPECTED.to_string())
}

fn chain_execute_error_to_code(err: &ChainError) -> Option<(TxApiErrorKind, &'static str)> {
    match err {
        ChainError::ExecFailed(exec) => {
            Some((TxApiErrorKind::Rejected, exec_error_to_code(exec.as_ref())))
        }
        _ => chain_submit_error_to_code(err),
    }
}

fn map_execute_chain_error(err: ChainError) -> ExecuteTxError {
    if let Some((kind, code)) = chain_execute_error_to_code(&err) {
        return match kind {
            TxApiErrorKind::InvalidArgument => ExecuteTxError::InvalidArgument(code.to_string()),
            TxApiErrorKind::Rejected => ExecuteTxError::Rejected(code.to_string()),
        };
    }
    tracing::error!(error = ?err, "execute transaction failed");
    ExecuteTxError::Internal(CODE_INTERNAL_UNEXPECTED.to_string())
}

fn map_execute_chain_result(
    result: Result<chain::ExecResult, chain::ChainError>,
) -> Result<super::ExecResultDto, ExecuteTxError> {
    let result = match result {
        Ok(value) => value,
        Err(err) => return Err(map_execute_chain_error(err)),
    };
    Ok(super::ExecResultDto {
        tx_id: result.tx_id.0.to_vec(),
        block_number: result.block_number,
        tx_index: result.tx_index,
        status: result.status,
        gas_used: result.gas_used,
        return_data: super::clamp_return_data(result.return_data),
    })
}

fn receipt_to_eth_view(receipt: ReceiptLike) -> super::EthReceiptView {
    let (eth_tx_hash, from, to, tx_type) = chain::get_tx_envelope(&receipt.tx_id)
        .and_then(|envelope| evm_db::chain_data::StoredTx::try_from(envelope).ok())
        .map(|stored| {
            let kind = stored.kind;
            let caller = match kind {
                evm_db::chain_data::TxKind::IcSynthetic => stored.caller_evm.unwrap_or([0u8; 20]),
                evm_db::chain_data::TxKind::EthSigned => [0u8; 20],
            };
            let decoded = evm_core::tx_decode::decode_tx_view(kind, caller, &stored.raw).ok();
            let eth_hash = if kind == evm_db::chain_data::TxKind::EthSigned {
                Some(hash::keccak256(&stored.raw).to_vec())
            } else {
                None
            };
            let from = decoded.as_ref().map(|v| v.from.to_vec());
            let to = decoded
                .as_ref()
                .and_then(|v| v.to.map(|addr| addr.to_vec()));
            let tx_type = decoded.as_ref().map(|v| v.tx_type);
            (eth_hash, from, to, tx_type)
        })
        .unwrap_or((None, None, None, None));
    let block_hash = chain::get_block(receipt.block_number).map(|block| block.block_hash.to_vec());
    super::EthReceiptView {
        tx_hash: receipt.tx_id.0.to_vec(),
        eth_tx_hash,
        block_hash,
        block_number: receipt.block_number,
        tx_index: receipt.tx_index,
        from,
        to,
        status: receipt.status,
        gas_used: receipt.gas_used,
        cumulative_gas_used: Some(receipt.gas_used),
        effective_gas_price: receipt.effective_gas_price,
        l1_data_fee: receipt.l1_data_fee,
        operator_fee: receipt.operator_fee,
        total_fee: receipt.total_fee,
        contract_address: receipt.contract_address.map(|v| v.to_vec()),
        tx_type,
        logs: receipt
            .logs
            .into_iter()
            .enumerate()
            .map(|(index, log)| super::EthReceiptLogView {
                address: log.address.as_slice().to_vec(),
                topics: log
                    .data
                    .topics()
                    .iter()
                    .map(|topic| topic.as_slice().to_vec())
                    .collect(),
                data: log.data.data.to_vec(),
                log_index: u32::try_from(index).unwrap_or(u32::MAX),
            })
            .collect(),
    }
}

fn prune_boundary_for_number(number: u64) -> Option<u64> {
    let pruned_before = with_state(|state| state.prune_state.get().pruned_before());
    match pruned_before {
        Some(pruned) if number <= pruned => Some(pruned),
        _ => None,
    }
}

fn receipt_lookup_status(tx_id: TxId) -> super::RpcReceiptLookupView {
    if let Some(receipt) = chain::get_receipt(&tx_id) {
        return super::RpcReceiptLookupView::Found(Box::new(receipt_to_eth_view(receipt)));
    }
    let pruned_before = with_state(|state| state.prune_state.get().pruned_before());
    let loc = chain::get_tx_loc(&tx_id);
    if let Some(loc) = loc {
        if loc.kind == evm_db::chain_data::TxLocKind::Included {
            if let Some(pruned) = pruned_before {
                if loc.block_number <= pruned {
                    return super::RpcReceiptLookupView::Pruned {
                        pruned_before_block: pruned,
                    };
                }
            }
        }
        return super::RpcReceiptLookupView::NotFound;
    }
    if let Some(pruned) = pruned_before {
        return super::RpcReceiptLookupView::PossiblyPruned {
            pruned_before_block: pruned,
        };
    }
    super::RpcReceiptLookupView::NotFound
}

fn sample_unwrap_request(
    status: UnwrapRequestStatus,
    error_code: Option<&str>,
    updated_at: u64,
) -> UnwrapDispatchRequest {
    UnwrapDispatchRequest {
        asset_id: vec![0x55u8; 10],
        amount: [0x66u8; 32],
        recipient: vec![0x77u8; 10],
        status,
        ledger_tx_id: None,
        error_code: error_code.map(str::to_string),
        updated_at,
        transfer_created_at_time: 0,
    }
}

fn sample_wrap_request(status: RequestStatus) -> WrapStoredRequest {
    WrapStoredRequest {
        caller: vec![1],
        asset_id: vec![2],
        amount: vec![0x11; 32],
        evm_recipient: vec![0x55; 20],
        gas_limit: 21_000,
        fee_ledger_canister: vec![7],
        max_fee_e8s: 9,
        quoted_gas_price_wei: 10,
        fee_created_at_time: 1,
        pull_created_at_time: 2,
        withdraw_created_at_time: 0,
        result: WrapRequestResult {
            status,
            pull_ledger_tx_id: None,
            mint_tx_id: None,
            error_code: None,
            withdrawn: false,
            withdraw_ledger_tx_id: None,
            withdraw_error_code: None,
            withdraw_in_progress: false,
            mint_failed_recoverable: false,
            fee_ledger_tx_id: Some(vec![6]),
            charged_fee_e8s: Some(7),
            charged_gas_price_wei: Some(8),
            stage: WrapRequestStage::FeeCollected,
            updated_at: 3,
            mint_nonce: None,
            mint_submitted_at_time: 0,
            mint_submit_status: MintSubmitStatus::NotSubmitted,
        },
    }
}

fn find_subsequence_positions(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return Vec::new();
    }
    haystack
        .windows(needle.len())
        .enumerate()
        .filter_map(|(idx, window)| if window == needle { Some(idx) } else { None })
        .collect()
}

fn unwrap_log_data(asset_id: &[u8], amount: [u8; 32], recipient: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + asset_id.len() + amount.len() + recipient.len());
    out.push(u8::try_from(asset_id.len()).expect("asset len"));
    out.extend_from_slice(asset_id);
    out.extend_from_slice(&amount);
    out.push(u8::try_from(recipient.len()).expect("recipient len"));
    out.extend_from_slice(recipient);
    out
}

fn native_withdraw_log_data(amount: [u8; 32], recipient: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(33 + recipient.len());
    out.extend_from_slice(&amount);
    out.push(u8::try_from(recipient.len()).expect("recipient len"));
    out.extend_from_slice(recipient);
    out
}

fn icp_update_log_data(target: &[u8], method: &str, arg: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + target.len() + method.len() + arg.len());
    out.push(u8::try_from(target.len()).expect("target len"));
    out.extend_from_slice(target);
    out.push(u8::try_from(method.len()).expect("method len"));
    out.extend_from_slice(method.as_bytes());
    out.extend_from_slice(&(arg.len() as u32).to_be_bytes());
    out.extend_from_slice(arg);
    out
}

fn test_icp_update_request(
    request_id: TxId,
    target: Vec<u8>,
    method: &str,
    status: IcpUpdateRequestStatus,
) -> IcpUpdateDispatchRequest {
    IcpUpdateDispatchRequest {
        request_id,
        tx_id: TxId([0x51u8; 32]),
        block_number: 7,
        tx_index: 1,
        log_index: 2,
        tx_kind: TxKind::IcSynthetic,
        evm_sender: [0x52u8; 20],
        ic_caller: Some(vec![0x53]),
        target,
        method: method.to_string(),
        arg: vec![0x44, 0x49, 0x44, 0x4c],
        status,
        reply: None,
        error_code: None,
        updated_at: 1,
        call_started_at_time: 0,
    }
}

#[test]
fn parse_submit_ic_tx_args_rejects_value_out_of_range() {
    let too_large = Nat::from_str(
        "115792089237316195423570985008687907853269984665640564039457584007913129639936",
    )
    .expect("nat parse");
    let err = parse_submit_ic_tx_args(SubmitIcTxArgsDto {
        to: Some(vec![0x11; 20]),
        from: None,
        value: too_large,
        gas_limit: 50_000,
        nonce: 0,
        max_fee_per_gas: Nat::from(2_000_000_000u64),
        max_priority_fee_per_gas: Nat::from(1_000_000_000u64),
        data: Vec::new(),
    })
    .expect_err("value out of range");
    match err {
        super::SubmitTxError::InvalidArgument(code) => {
            assert_eq!(code, "arg.value_out_of_range")
        }
        _ => panic!("unexpected error"),
    }
}

#[test]
fn basic_boundary_helpers_enforce_size_contracts() {
    let clamp_cases = [
        ("allow_limit", vec![7u8; MAX_RETURN_DATA], true),
        ("reject_oversize", vec![0u8; MAX_RETURN_DATA + 1], false),
    ];
    for (case, input, expect_some) in clamp_cases {
        let out = clamp_return_data(input.clone());
        assert_eq!(out.is_some(), expect_some, "{case}");
        if expect_some {
            assert_eq!(out, Some(input), "{case}");
        }
    }

    let tx_id_cases = [
        ("reject_short", vec![1u8; 31], false),
        ("accept_32", vec![9u8; 32], true),
    ];
    for (case, input, expect_some) in tx_id_cases {
        let out = tx_id_from_bytes(input.clone());
        assert_eq!(out.is_some(), expect_some, "{case}");
        if expect_some {
            assert_eq!(out.expect("tx_id").0.to_vec(), input, "{case}");
        }
    }
}

#[test]
fn exec_error_codes_match_fixed_pattern() {
    let inputs = [
        Some(ExecError::Decode(
            evm_core::tx_decode::DecodeError::InvalidRlp,
        )),
        Some(ExecError::TxError(OpTransactionError::TxBuildFailed)),
        Some(ExecError::TxError(OpTransactionError::TxRejectedByPolicy)),
        Some(ExecError::TxError(OpTransactionError::TxPrecheckFailed)),
        Some(ExecError::TxError(OpTransactionError::TxExecutionFailed)),
        Some(ExecError::Revert),
        Some(ExecError::EvmHalt(OpHaltReason::OutOfGas)),
        Some(ExecError::EvmHalt(OpHaltReason::InvalidOpcode)),
        Some(ExecError::EvmHalt(OpHaltReason::StackOverflow)),
        Some(ExecError::EvmHalt(OpHaltReason::StackUnderflow)),
        Some(ExecError::EvmHalt(OpHaltReason::InvalidJump)),
        Some(ExecError::EvmHalt(
            OpHaltReason::StateChangeDuringStaticCall,
        )),
        Some(ExecError::EvmHalt(OpHaltReason::PrecompileError)),
        Some(ExecError::EvmHalt(OpHaltReason::Unknown)),
        Some(ExecError::InvalidGasFee),
        Some(ExecError::ResultTooLarge),
        Some(ExecError::InstructionBudgetExceeded),
        Some(ExecError::ExecutionFailed),
        None,
    ];

    for err in inputs.iter() {
        let code = exec_error_to_code(err.as_ref());
        assert!(code.starts_with("exec."));
        assert!(is_machine_code(code), "unexpected code: {code}");
        assert!(!code.contains('{'));
        assert!(!code.contains('}'));
        assert!(!code.contains(':'));
    }
}

#[test]
fn pr8_submit_error_code_mapping_matches_expected_set() {
    let table = [
        (ChainError::TxTooLarge, ("arg.tx_too_large", true)),
        (ChainError::DecodeFailed, ("arg.decode_failed", true)),
        (
            ChainError::AddressDerivationFailed,
            ("arg.principal_to_evm_derivation_failed", true),
        ),
        (
            ChainError::UnsupportedTxKind,
            ("arg.unsupported_tx_kind", true),
        ),
        (ChainError::TxAlreadySeen, ("submit.tx_already_seen", false)),
        (ChainError::InvalidFee, ("submit.invalid_fee", false)),
        (ChainError::NonceTooLow, ("submit.nonce_too_low", false)),
        (ChainError::NonceGap, ("submit.nonce_gap", false)),
        (ChainError::NonceConflict, ("submit.nonce_conflict", false)),
        (ChainError::QueueFull, ("submit.queue_full", false)),
        (
            ChainError::SenderQueueFull,
            ("submit.sender_queue_full", false),
        ),
        (
            ChainError::PrincipalQueueFull,
            ("submit.principal_queue_full", false),
        ),
        (
            ChainError::DecodeRateLimited,
            ("submit.decode_rate_limited", false),
        ),
    ];
    for (input, (expected_code, expected_invalid_arg)) in table {
        let (kind, code) = chain_submit_error_to_code(&input).expect("code mapping");
        assert_eq!(code, expected_code);
        assert!(is_machine_code(code));
        match kind {
            TxApiErrorKind::InvalidArgument => assert!(expected_invalid_arg),
            TxApiErrorKind::Rejected => assert!(!expected_invalid_arg),
        }
    }
}

#[test]
fn exec_error_to_code_matches_expected_set() {
    let cases = [
        ("revert", Some(ExecError::Revert), "exec.revert"),
        (
            "halt_unknown",
            Some(ExecError::EvmHalt(OpHaltReason::Unknown)),
            "exec.halt.unknown",
        ),
        (
            "tx_build_failed",
            Some(ExecError::TxError(OpTransactionError::TxBuildFailed)),
            "exec.tx.build_failed",
        ),
        (
            "result_too_large",
            Some(ExecError::ResultTooLarge),
            "exec.result.too_large",
        ),
        (
            "instruction_budget",
            Some(ExecError::InstructionBudgetExceeded),
            "exec.budget.instruction_exceeded",
        ),
    ];
    for (case, err, expected) in cases {
        assert_eq!(exec_error_to_code(err.as_ref()), expected, "{case}");
    }
}

#[test]
fn status_zero_exec_result_is_not_rejected() {
    let result = map_execute_chain_result(Ok(ExecResult {
        tx_id: TxId([0u8; 32]),
        block_number: 1,
        tx_index: 0,
        status: 0,
        gas_used: 21_000,
        return_data: Vec::new(),
        final_status: "Revert".to_string(),
    }))
    .expect("status=0 should still be Ok");
    assert_eq!(result.status, 0);
}

#[test]
fn exec_failed_maps_to_rejected_exec_code() {
    let err = map_execute_chain_result(Err(ChainError::ExecFailed(Some(ExecError::Revert))))
        .expect_err("exec failed should be rejected");
    match err {
        ExecuteTxError::Rejected(code) => assert_eq!(code, "exec.revert"),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn chain_error_mapping_covers_expected_api_shapes() {
    let execute_cases = [
        (
            "decode_failed",
            ChainError::DecodeFailed,
            ExecuteTxError::InvalidArgument("arg.decode_failed".to_string()),
        ),
        (
            "derivation_failed",
            ChainError::AddressDerivationFailed,
            ExecuteTxError::InvalidArgument("arg.principal_to_evm_derivation_failed".to_string()),
        ),
        (
            "precompile_error",
            ChainError::ExecFailed(Some(ExecError::EvmHalt(OpHaltReason::PrecompileError))),
            ExecuteTxError::Rejected("exec.halt.precompile_error".to_string()),
        ),
    ];
    for (case, err, expected) in execute_cases {
        let actual = map_execute_chain_result(Err(err)).expect_err(case);
        match (actual, expected) {
            (
                ExecuteTxError::InvalidArgument(actual_code),
                ExecuteTxError::InvalidArgument(expected_code),
            ) => assert_eq!(actual_code, expected_code, "{case}"),
            (ExecuteTxError::Rejected(actual_code), ExecuteTxError::Rejected(expected_code)) => {
                assert_eq!(actual_code, expected_code, "{case}")
            }
            (left, right) => panic!("unexpected mismatch for {case}: {left:?} vs {right:?}"),
        }
    }

    let submit_err = map_submit_chain_error(ChainError::QueueEmpty, "test_submit");
    match submit_err {
        super::SubmitTxError::Internal(code) => {
            assert_eq!(code, "internal.unexpected")
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn inspect_allowlist_accepts_known_methods() {
    assert!(inspect_payload_limit_for_method("submit_ic_tx").is_some());
    assert!(inspect_payload_limit_for_method("set_pruning_enabled").is_some());
    assert!(inspect_payload_limit_for_method("set_query_instruction_soft_limit").is_none());
    assert!(inspect_payload_limit_for_method("set_update_instruction_soft_limit").is_none());
    assert!(inspect_payload_limit_for_method("set_precompile_gas_ratio").is_none());
    #[cfg(feature = "precompile-profile-admin")]
    assert!(inspect_payload_limit_for_method("clear_precompile_profile").is_some());
    #[cfg(feature = "precompile-profile-admin")]
    assert!(inspect_payload_limit_for_method("profile_precompile_call").is_some());
    #[cfg(not(feature = "precompile-profile-admin"))]
    assert!(inspect_payload_limit_for_method("clear_precompile_profile").is_none());
    #[cfg(not(feature = "precompile-profile-admin"))]
    assert!(inspect_payload_limit_for_method("profile_precompile_call").is_none());
}

#[test]
fn inspect_allowlist_rejects_unknown_methods() {
    assert!(inspect_payload_limit_for_method("unknown_method").is_none());
}

#[test]
fn inspect_allowlist_matches_did_updates() {
    let did_methods = did_update_methods();
    for method in did_methods.iter() {
        assert!(
            inspect_payload_limit_for_method(method).is_some(),
            "did update method is missing in inspect allowlist: {method}"
        );
    }
    for method in inspect_allowlist_methods().iter() {
        assert!(
            did_methods.contains(*method),
            "inspect allowlist method is missing in did: {method}"
        );
    }
}

#[test]
fn reject_anonymous_principal_blocks_anonymous() {
    let out = reject_anonymous_principal(Principal::anonymous());
    assert_eq!(out, Some("auth.anonymous_forbidden".to_string()));
}

#[test]
fn reject_anonymous_principal_allows_non_anonymous() {
    let principal = Principal::self_authenticating(b"wrapper-test-caller");
    let out = reject_anonymous_principal(principal);
    assert_eq!(out, None);
}

#[test]
fn reject_write_reason_stops_on_needs_migration() {
    init_stable_state();
    set_schema_migration_state(SchemaMigrationState::done());
    set_needs_migration(true);
    let reason = reject_write_reason().expect("needs_migration should block writes");
    assert_eq!(reason, "ops.write.needs_migration");
}

fn set_migration_not_pending_for_test() {
    set_needs_migration(false);
    set_schema_migration_state(SchemaMigrationState::done());
    evm_db::stable_state::with_state_mut(|state| {
        let mut meta = *state.state_root_meta.get();
        meta.initialized = true;
        state.state_root_meta.set(meta);
        let mut migration = *state.state_root_migration.get();
        migration.phase = MigrationPhase::Done;
        migration.cursor = 0;
        migration.last_error = 0;
        state.state_root_migration.set(migration);
    });
}

#[test]
fn producer_gate_cycle_critical_reason_is_stable() {
    let reason = ic_evm_ops::reject_write_reason_with_mode_provider(false, || OpsMode::Critical)
        .expect("critical mode must reject");
    assert_eq!(reason, "ops.write.cycle_critical");
}

#[test]
fn migration_pending_does_not_advance_schema_migration_state() {
    init_stable_state();
    set_migration_not_pending_for_test();
    set_schema_migration_state(SchemaMigrationState {
        phase: SchemaMigrationPhase::Init,
        cursor: 0,
        from_version: current_schema_version(),
        to_version: current_schema_version(),
        last_error: 0,
        cursor_key_set: false,
        cursor_key: [0u8; 32],
    });

    let before = schema_migration_state();
    assert_eq!(before.phase, SchemaMigrationPhase::Init);
    let pending = migration_pending();
    assert!(!pending);
    let after = schema_migration_state();
    assert_eq!(after.phase, SchemaMigrationPhase::Init);
    assert_eq!(after.cursor, before.cursor);
}

#[test]
fn cycle_observer_migration_tick_condition_matches_pending_state() {
    assert!(should_run_cycle_observer_migration_tick(true));
    assert!(!should_run_cycle_observer_migration_tick(false));
}

#[test]
fn cycle_observer_schedule_mining_condition_is_explicit() {
    assert!(should_schedule_mining_after_cycle_observer(
        OpsMode::Normal,
        false
    ));
    assert!(should_schedule_mining_after_cycle_observer(
        OpsMode::Low,
        false
    ));
    assert!(!should_schedule_mining_after_cycle_observer(
        OpsMode::Critical,
        false
    ));
    assert!(!should_schedule_mining_after_cycle_observer(
        OpsMode::Normal,
        true
    ));
}

#[test]
fn reset_mining_schedule_after_upgrade_clears_stale_flag() {
    init_stable_state();
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.mining_scheduled = true;
        state.chain_state.set(chain_state);
    });
    super::reset_mining_schedule_after_upgrade();
    evm_db::stable_state::with_state(|state| {
        assert!(!state.chain_state.get().mining_scheduled);
    });
}

#[test]
fn recover_unwrap_dispatch_after_upgrade_requeues_dispatching_requests() {
    init_stable_state();
    let request_id = TxId([0x61u8; 32]);
    with_state_mut(|state| {
        state.unwrap_requests.insert(
            request_id,
            sample_unwrap_request(UnwrapRequestStatus::Dispatching, None, 1),
        );
    });

    assert!(super::recover_unwrap_dispatch_state_after_upgrade(123));
    with_state(|state| {
        let req = state.unwrap_requests.get(&request_id).expect("request");
        assert_eq!(req.status, UnwrapRequestStatus::Queued);
        assert_eq!(req.updated_at, 123);
        assert_eq!(state.unwrap_dispatch_queue.len(), 1);
    });
}

#[test]
fn recover_unwrap_dispatch_after_upgrade_preserves_existing_queue_without_duplicates() {
    init_stable_state();
    let request_id = TxId([0x62u8; 32]);
    with_state_mut(|state| {
        let mut meta = *state.unwrap_dispatch_meta.get();
        let seq = meta.push();
        state.unwrap_dispatch_meta.set(meta);
        state.unwrap_dispatch_queue.insert(seq, request_id);
        state.unwrap_requests.insert(
            request_id,
            sample_unwrap_request(UnwrapRequestStatus::Queued, None, 1),
        );
    });

    assert!(super::recover_unwrap_dispatch_state_after_upgrade(222));
    with_state(|state| {
        let req = state.unwrap_requests.get(&request_id).expect("request");
        assert_eq!(req.status, UnwrapRequestStatus::Queued);
        assert_eq!(req.updated_at, 1);
        assert_eq!(state.unwrap_dispatch_queue.len(), 1);
    });
}

#[test]
fn recover_unwrap_dispatch_after_upgrade_ignores_terminal_requests() {
    init_stable_state();
    let dispatched = TxId([0x63u8; 32]);
    let failed = TxId([0x64u8; 32]);
    with_state_mut(|state| {
        state.unwrap_requests.insert(
            dispatched,
            sample_unwrap_request(UnwrapRequestStatus::Dispatched, None, 11),
        );
        state.unwrap_requests.insert(
            failed,
            sample_unwrap_request(
                UnwrapRequestStatus::DispatchFailed,
                Some("wrap.call_failed:oops"),
                12,
            ),
        );
    });

    assert!(!super::recover_unwrap_dispatch_state_after_upgrade(333));
    with_state(|state| {
        assert_eq!(state.unwrap_dispatch_queue.len(), 0);
        assert_eq!(
            state.unwrap_requests.get(&dispatched).map(|req| req.status),
            Some(UnwrapRequestStatus::Dispatched)
        );
        assert_eq!(
            state.unwrap_requests.get(&failed).map(|req| req.status),
            Some(UnwrapRequestStatus::DispatchFailed)
        );
    });
}

#[test]
fn pending_wrap_submission_rejects_concurrent_same_request() {
    init_stable_state();
    let request_id = TxId([0x6au8; 32]);
    let caller = Principal::self_authenticating(b"wrap-pending-caller");
    let other = Principal::self_authenticating(b"wrap-pending-other");

    super::reserve_wrap_pending_submission(request_id, caller).expect("reserve");
    assert_eq!(
        super::reserve_wrap_pending_submission(request_id, caller).expect_err("same caller"),
        "request.in_progress"
    );
    assert_eq!(
        super::reserve_wrap_pending_submission(request_id, other).expect_err("other caller"),
        "request.idempotency_mismatch"
    );

    super::clear_wrap_pending_submission(request_id);
    super::reserve_wrap_pending_submission(request_id, caller).expect("reserve after clear");
}

#[test]
fn pending_wrap_submission_replaces_decode_failure_placeholder() {
    init_stable_state();
    let request_id = TxId([0x6bu8; 32]);
    let caller = Principal::self_authenticating(b"wrap-pending-caller");
    with_state_mut(|state| {
        state.wrap_pending_submissions.insert(
            request_id,
            WrapPendingSubmission {
                caller: Vec::new(),
                request_id: Vec::new(),
            },
        );
    });

    super::reserve_wrap_pending_submission(request_id, caller).expect("reserve after placeholder");

    let stored = with_state(|state| state.wrap_pending_submissions.get(&request_id))
        .expect("pending submission");
    assert_eq!(stored.caller, caller.as_slice());
    assert_eq!(stored.request_id, request_id.0);
}

#[test]
fn recover_wrap_worker_after_upgrade_requeues_active_requests_without_duplicates() {
    init_stable_state();
    let queued_existing = TxId([0x6bu8; 32]);
    let queued_missing = TxId([0x6cu8; 32]);
    let running_missing = TxId([0x6du8; 32]);
    let succeeded = TxId([0x6eu8; 32]);

    with_state_mut(|state| {
        let mut meta = *state.wrap_queue_meta.get();
        let seq = meta.push();
        state.wrap_queue_meta.set(meta);
        state.wrap_queue.insert(seq, queued_existing);
        state
            .wrap_requests
            .insert(queued_existing, sample_wrap_request(RequestStatus::Queued));
        state
            .wrap_requests
            .insert(queued_missing, sample_wrap_request(RequestStatus::Queued));
        state
            .wrap_requests
            .insert(running_missing, sample_wrap_request(RequestStatus::Running));
        state
            .wrap_requests
            .insert(succeeded, sample_wrap_request(RequestStatus::Succeeded));
    });

    assert!(super::recover_wrap_worker_state_after_upgrade());
    with_state(|state| {
        let queued = state
            .wrap_queue
            .iter()
            .map(|entry| entry.value())
            .collect::<BTreeSet<_>>();
        assert_eq!(queued.len(), 3);
        assert!(queued.contains(&queued_existing));
        assert!(queued.contains(&queued_missing));
        assert!(queued.contains(&running_missing));
        assert!(!queued.contains(&succeeded));
        assert_eq!(
            state
                .wrap_requests
                .get(&running_missing)
                .expect("running")
                .result
                .status,
            RequestStatus::Queued
        );
    });
}

#[test]
fn finalize_unwrap_dispatch_attempt_keeps_call_failed_out_of_queue() {
    init_stable_state();
    let request_id = TxId([0x65u8; 32]);
    with_state_mut(|state| {
        state.unwrap_requests.insert(
            request_id,
            sample_unwrap_request(UnwrapRequestStatus::Dispatching, Some("old"), 1),
        );
    });

    super::finalize_unwrap_dispatch_attempt(
        request_id,
        444,
        super::AppliedUnwrapDispatchOutcome {
            status: UnwrapRequestStatus::DispatchFailed,
            ledger_tx_id: None,
            error_code: Some("wrap.call_failed:transport".to_string()),
        },
    );

    with_state(|state| {
        let req = state.unwrap_requests.get(&request_id).expect("request");
        assert_eq!(req.status, UnwrapRequestStatus::DispatchFailed);
        assert_eq!(
            req.error_code,
            Some("wrap.call_failed:transport".to_string())
        );
        assert_eq!(req.updated_at, 444);
        assert_eq!(state.unwrap_dispatch_queue.len(), 0);
    });
}

#[test]
fn finalize_unwrap_dispatch_attempt_keeps_terminal_failure_out_of_queue() {
    init_stable_state();
    let request_id = TxId([0x66u8; 32]);
    with_state_mut(|state| {
        state.unwrap_requests.insert(
            request_id,
            sample_unwrap_request(UnwrapRequestStatus::Dispatching, None, 1),
        );
    });

    super::finalize_unwrap_dispatch_attempt(
        request_id,
        555,
        super::AppliedUnwrapDispatchOutcome {
            status: UnwrapRequestStatus::DispatchFailed,
            ledger_tx_id: None,
            error_code: Some("wrap.submit_failed:request.invalid".to_string()),
        },
    );

    with_state(|state| {
        let req = state.unwrap_requests.get(&request_id).expect("request");
        assert_eq!(req.status, UnwrapRequestStatus::DispatchFailed);
        assert_eq!(
            req.error_code,
            Some("wrap.submit_failed:request.invalid".to_string())
        );
        assert_eq!(req.updated_at, 555);
        assert_eq!(state.unwrap_dispatch_queue.len(), 0);
    });
}

fn build_ic_synthetic_tx_input_for_test(
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
) -> IcSyntheticTxInput {
    IcSyntheticTxInput {
        to: Some([0x11u8; 20]),
        value: [0u8; 32],
        gas_limit: 21_000,
        nonce,
        max_fee_per_gas,
        max_priority_fee_per_gas,
        data: Vec::new(),
    }
}

fn no_timer_for_test(_interval_ms: u64) {}

fn no_reject_for_test() -> Option<String> {
    None
}

fn no_schedule_for_test() {}

fn assert_included_receipt_index_location_links(tx_id: TxId, block_number: u64, tx_index: u32) {
    let loc = chain::get_tx_loc(&tx_id).expect("included tx loc");
    assert_eq!(loc.kind, TxLocKind::Included);
    assert_eq!(loc.block_number, block_number);
    assert_eq!(loc.tx_index, tx_index);

    let block = chain::get_block(block_number).expect("included block");
    assert_eq!(block.tx_ids.get(tx_index as usize), Some(&tx_id));

    evm_db::stable_state::with_state(|state| {
        let index_ptr = state.tx_index.get(&tx_id).expect("tx index ptr");
        let index_bytes = state.blob_store.read(&index_ptr).expect("tx index bytes");
        let index = TxIndexEntry::from_bytes(Cow::Owned(index_bytes));
        assert_eq!(index.block_number, block_number);
        assert_eq!(index.tx_index, tx_index);

        let receipt_ptr = state.receipts.get(&tx_id).expect("receipt ptr");
        let receipt_bytes = state.blob_store.read(&receipt_ptr).expect("receipt bytes");
        let receipt = ReceiptLike::from_bytes(Cow::Owned(receipt_bytes));
        assert_eq!(receipt.tx_id, tx_id);
        assert_eq!(receipt.block_number, block_number);
        assert_eq!(receipt.tx_index, tx_index);
    });
}

#[test]
fn gateway_submit_ic_tx_adapter_preserves_queue_and_receipt_invariants() {
    init_stable_state();
    set_migration_not_pending_for_test();

    let caller = Principal::self_authenticating(b"gateway-submit-caller");
    let canister = Principal::self_authenticating(b"gateway-submit-canister");
    let (max_fee_per_gas, max_priority_fee_per_gas) = evm_db::stable_state::with_state(|state| {
        let chain_state = *state.chain_state.get();
        let min_priority = u128::from(chain_state.min_priority_fee);
        let required_max_fee = u128::from(chain_state.base_fee)
            .saturating_add(min_priority)
            .max(u128::from(chain_state.min_gas_price));
        (required_max_fee, min_priority)
    });
    let args = SubmitIcTxArgsDto {
        to: Some(vec![0x11; 20]),
        from: None,
        value: Nat::from(0u8),
        gas_limit: 50_000,
        nonce: 0,
        max_fee_per_gas: Nat::from(max_fee_per_gas),
        max_priority_fee_per_gas: Nat::from(max_priority_fee_per_gas),
        data: Vec::new(),
    };
    let tx = parse_submit_ic_tx_args(args).expect("parse submit_ic_tx args");
    let caller_evm =
        hash::derive_evm_address_from_principal(caller.as_slice()).expect("caller evm");
    chain::credit_balance(caller_evm, 1_000_000_000_000_000_000).expect("fund caller");
    let expected_tx_id =
        super::derive_ic_synthetic_tx_id(caller.as_slice(), canister.as_slice(), &tx, caller_evm);

    let returned_tx_id = super::submit_ic_tx_internal_with_canister_and_scheduler(
        caller.as_slice().to_vec(),
        canister.as_slice().to_vec(),
        "submit_ic_tx",
        tx,
        no_schedule_for_test,
    )
    .expect("submit_ic_tx adapter");
    assert_eq!(returned_tx_id, expected_tx_id.0.to_vec());

    let queued_loc = chain::get_tx_loc(&expected_tx_id).expect("queued tx loc");
    assert_eq!(queued_loc.kind, evm_db::chain_data::TxLocKind::Queued);
    assert!(matches!(
        super::get_receipt(returned_tx_id.clone()),
        Err(super::LookupError::Pending)
    ));
    evm_db::stable_state::with_state(|state| {
        assert!(state.tx_store.get(&expected_tx_id).is_some());
        assert!(state.receipts.get(&expected_tx_id).is_none());
    });

    let previous_head = chain::get_head_number();
    let outcome = chain::produce_block(1).expect("produce block");
    assert_eq!(outcome.block.tx_ids, vec![expected_tx_id]);
    assert_eq!(chain::get_head_number(), previous_head + 1);

    let included_loc = chain::get_tx_loc(&expected_tx_id).expect("included tx loc");
    assert_eq!(included_loc.kind, evm_db::chain_data::TxLocKind::Included);
    assert_eq!(included_loc.block_number, outcome.block.number);
    assert_eq!(included_loc.tx_index, 0);
    let receipt = super::get_receipt(returned_tx_id).expect("receipt view");
    assert_eq!(receipt.tx_id, expected_tx_id.0.to_vec());
    assert_eq!(receipt.block_number, outcome.block.number);
    assert_eq!(receipt.tx_index, 0);
    assert_included_receipt_index_location_links(expected_tx_id, outcome.block.number, 0);
}

#[test]
fn replacement_old_tx_is_not_staged_or_included() {
    init_stable_state();
    set_migration_not_pending_for_test();

    let caller = Principal::self_authenticating(b"gateway-replacement-caller");
    let canister = Principal::self_authenticating(b"gateway-replacement-canister");
    let (max_fee_per_gas, max_priority_fee_per_gas) = evm_db::stable_state::with_state(|state| {
        let chain_state = *state.chain_state.get();
        let min_priority = u128::from(chain_state.min_priority_fee);
        let required_max_fee = u128::from(chain_state.base_fee)
            .saturating_add(min_priority)
            .max(u128::from(chain_state.min_gas_price));
        (required_max_fee, min_priority)
    });
    let caller_evm =
        hash::derive_evm_address_from_principal(caller.as_slice()).expect("caller evm");
    chain::credit_balance(caller_evm, 1_000_000_000_000_000_000).expect("fund caller");

    let first_tx =
        build_ic_synthetic_tx_input_for_test(0, max_fee_per_gas, max_priority_fee_per_gas);
    let first_tx_id = super::derive_ic_synthetic_tx_id(
        caller.as_slice(),
        canister.as_slice(),
        &first_tx,
        caller_evm,
    );
    super::submit_ic_tx_internal_with_canister_and_scheduler(
        caller.as_slice().to_vec(),
        canister.as_slice().to_vec(),
        "submit_ic_tx",
        first_tx,
        no_schedule_for_test,
    )
    .expect("first submit");

    let replacement_tx = build_ic_synthetic_tx_input_for_test(
        0,
        max_fee_per_gas.saturating_add(10_000),
        max_priority_fee_per_gas.saturating_add(10_000),
    );
    let replacement_tx_id = super::derive_ic_synthetic_tx_id(
        caller.as_slice(),
        canister.as_slice(),
        &replacement_tx,
        caller_evm,
    );
    super::submit_ic_tx_internal_with_canister_and_scheduler(
        caller.as_slice().to_vec(),
        canister.as_slice().to_vec(),
        "submit_ic_tx",
        replacement_tx,
        no_schedule_for_test,
    )
    .expect("replacement submit");

    evm_db::stable_state::with_state(|state| {
        assert_eq!(state.pending_current_by_sender.len(), 1);
        assert!(state.tx_store.get(&first_tx_id).is_none());
        assert!(state
            .ready_queue
            .iter()
            .all(|entry| entry.value() != first_tx_id));
        assert!(state
            .ready_by_seq
            .iter()
            .all(|entry| entry.value() != first_tx_id));
        assert!(state
            .pending_by_sender_nonce
            .iter()
            .all(|entry| { entry.value() == replacement_tx_id && entry.value() != first_tx_id }));
    });
    assert_eq!(
        chain::get_tx_loc(&first_tx_id).map(|loc| loc.kind),
        Some(TxLocKind::Dropped)
    );

    let outcome = chain::produce_block(1).expect("produce replacement block");
    assert_eq!(outcome.block.tx_ids, vec![replacement_tx_id]);
    assert!(!outcome.block.tx_ids.contains(&first_tx_id));
    assert!(chain::get_receipt(&first_tx_id).is_none());
    assert_included_receipt_index_location_links(replacement_tx_id, outcome.block.number, 0);
}

#[test]
fn mining_tick_stops_on_empty_queue_and_restarts_after_tx_arrival() {
    init_stable_state();
    set_migration_not_pending_for_test();
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.auto_production_enabled = true;
        chain_state.mining_scheduled = true;
        chain_state.is_producing = false;
        state.chain_state.set(chain_state);
    });

    super::mining_tick_with_timer(no_timer_for_test, no_reject_for_test);
    evm_db::stable_state::with_state(|state| {
        assert_eq!(state.ready_queue.len(), 0);
        assert!(!state.chain_state.get().mining_scheduled);
        assert!(!state.chain_state.get().is_producing);
    });

    let caller = Principal::self_authenticating(b"mining-tick-resume-caller");
    let canister = Principal::self_authenticating(b"mining-tick-resume-canister");
    let (max_fee_per_gas, max_priority_fee_per_gas) = evm_db::stable_state::with_state(|state| {
        let chain_state = *state.chain_state.get();
        let min_priority = u128::from(chain_state.min_priority_fee);
        let base_fee = u128::from(chain_state.base_fee);
        let min_gas_price = u128::from(chain_state.min_gas_price);
        let required_max_fee = base_fee.saturating_add(min_priority).max(min_gas_price);
        (required_max_fee, min_priority)
    });
    let tx_id = evm_core::chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: caller.as_slice().to_vec(),
        canister_id: canister.as_slice().to_vec(),
        tx: build_ic_synthetic_tx_input_for_test(0, max_fee_per_gas, max_priority_fee_per_gas),
    })
    .expect("submit_ic_tx should succeed");
    evm_db::stable_state::with_state(|state| {
        assert!(state.seen_tx.get(&tx_id).is_some());
        assert!(!state.ready_queue.is_empty());
        assert!(!state.chain_state.get().mining_scheduled);
    });

    super::schedule_mining_with_timer(no_timer_for_test, no_reject_for_test);
    evm_db::stable_state::with_state(|state| {
        assert!(state.chain_state.get().mining_scheduled);
    });
}

#[test]
fn mining_tick_does_not_reschedule_after_dropping_non_executable_tx() {
    init_stable_state();
    set_migration_not_pending_for_test();
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.auto_production_enabled = true;
        chain_state.mining_scheduled = true;
        chain_state.is_producing = false;
        state.chain_state.set(chain_state);
    });

    let caller = Principal::self_authenticating(b"mining-drop-caller");
    let canister = Principal::self_authenticating(b"mining-drop-canister");
    let (max_fee_per_gas, max_priority_fee_per_gas) = evm_db::stable_state::with_state(|state| {
        let chain_state = *state.chain_state.get();
        let min_priority = u128::from(chain_state.min_priority_fee);
        let base_fee = u128::from(chain_state.base_fee);
        let min_gas_price = u128::from(chain_state.min_gas_price);
        let required_max_fee = base_fee.saturating_add(min_priority).max(min_gas_price);
        (required_max_fee, min_priority)
    });
    let tx_id = evm_core::chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: caller.as_slice().to_vec(),
        canister_id: canister.as_slice().to_vec(),
        tx: build_ic_synthetic_tx_input_for_test(0, max_fee_per_gas, max_priority_fee_per_gas),
    })
    .expect("submit_ic_tx should succeed");

    // 直前に最低ガス価格を引き上げ、queue内txを「実行不能」にする。
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.min_gas_price = u64::MAX;
        state.chain_state.set(chain_state);
    });

    super::mining_tick_with_timer(no_timer_for_test, no_reject_for_test);
    evm_db::stable_state::with_state(|state| {
        assert_eq!(state.ready_queue.len(), 0);
        assert!(state.tx_store.get(&tx_id).is_none());
        assert!(!state.chain_state.get().mining_scheduled);
        assert!(!state.chain_state.get().is_producing);
    });
}

#[test]
fn inspect_payload_limit_applies_per_method() {
    let tx_limit = inspect_payload_limit_for_method("submit_ic_tx").expect("tx limit");
    let manage_limit = inspect_payload_limit_for_method("set_pruning_enabled")
        .expect("manage limit should be configured");
    assert!(manage_limit > tx_limit);
    assert_eq!(
        inspect_payload_limit_for_method("rpc_eth_send_raw_transaction"),
        Some(tx_limit)
    );
    assert_eq!(inspect_payload_limit_for_method("unknown_method"), None);
}

#[test]
fn inspect_policy_table_has_unique_methods() {
    let mut methods = BTreeSet::new();
    for policy in INSPECT_METHOD_POLICIES {
        let inserted = methods.insert(policy.method);
        assert!(
            inserted,
            "duplicate inspect policy method: {}",
            policy.method
        );
    }
}

#[test]
fn inspect_policy_allowed_and_limit_are_consistent() {
    for method in inspect_allowlist_methods() {
        assert!(
            inspect_payload_limit_for_method(method).is_some(),
            "payload limit missing for method: {method}"
        );
        assert!(inspect_policy_for_method(method).is_some());
    }
    assert!(inspect_payload_limit_for_method("unknown_method").is_none());
}

#[test]
fn inspect_lightweight_tx_guard_rejects_empty_raw_tx() {
    assert!(!inspect_lightweight_tx_guard_with_payload(
        "rpc_eth_send_raw_transaction",
        encode_one(Vec::<u8>::new()).expect("encode")
    ));
}

#[test]
fn inspect_lightweight_tx_guard_rejects_unsupported_typed_prefix() {
    assert!(!inspect_lightweight_tx_guard_with_payload(
        "rpc_eth_send_raw_transaction",
        encode_one(vec![0x03u8, 0x01]).expect("encode")
    ));
    assert!(!inspect_lightweight_tx_guard_with_payload(
        "rpc_eth_send_raw_transaction",
        encode_one(vec![0x04u8, 0x01]).expect("encode")
    ));
}

#[test]
fn inspect_lightweight_tx_guard_accepts_supported_payload() {
    assert!(inspect_lightweight_tx_guard_with_payload(
        "rpc_eth_send_raw_transaction",
        encode_one(vec![0x02u8, 0x01]).expect("encode")
    ));
    assert!(inspect_lightweight_tx_guard("set_pruning_enabled"));
}

fn inspect_lightweight_tx_guard_with_payload(method: &str, payload: Vec<u8>) -> bool {
    if method != "rpc_eth_send_raw_transaction" {
        return true;
    }
    let tx = match candid::decode_one::<Vec<u8>>(&payload) {
        Ok(value) => value,
        Err(_) => return false,
    };
    if tx.is_empty() {
        return false;
    }
    let first = tx[0];
    first != 0x03 && first != 0x04
}

#[test]
fn prune_boundary_for_number_returns_boundary_only_for_pruned_range() {
    init_stable_state();
    evm_db::stable_state::with_state_mut(|state| {
        let mut prune_state = *state.prune_state.get();
        prune_state.set_pruned_before(10);
        state.prune_state.set(prune_state);
    });
    assert_eq!(prune_boundary_for_number(10), Some(10));
    assert_eq!(prune_boundary_for_number(11), None);
}

#[test]
fn receipt_lookup_status_returns_possibly_pruned_when_loc_is_gone() {
    init_stable_state();
    let tx_id = TxId([0x44; 32]);
    evm_db::stable_state::with_state_mut(|state| {
        let mut prune_state = *state.prune_state.get();
        prune_state.set_pruned_before(7);
        state.prune_state.set(prune_state);
    });
    let out = receipt_lookup_status(tx_id);
    match out {
        super::RpcReceiptLookupView::PossiblyPruned {
            pruned_before_block,
        } => {
            assert_eq!(pruned_before_block, 7);
        }
        other => panic!("unexpected status: {other:?}"),
    }
}

#[test]
fn receipt_lookup_status_returns_pruned_when_loc_indicates_included_before_boundary() {
    init_stable_state();
    let tx_id = TxId([0x55; 32]);
    evm_db::stable_state::with_state_mut(|state| {
        state.tx_locs.insert(tx_id, TxLoc::included(5, 0));
        let mut prune_state = *state.prune_state.get();
        prune_state.set_pruned_before(8);
        state.prune_state.set(prune_state);
    });
    let out = receipt_lookup_status(tx_id);
    match out {
        super::RpcReceiptLookupView::Pruned {
            pruned_before_block,
        } => {
            assert_eq!(pruned_before_block, 8);
        }
        other => panic!("unexpected status: {other:?}"),
    }
}

#[test]
fn get_ops_status_reports_error_counters() {
    init_stable_state();
    let before_mining = MINING_ERROR_COUNT.load(std::sync::atomic::Ordering::Relaxed);
    let before_prune = PRUNE_ERROR_COUNT.load(std::sync::atomic::Ordering::Relaxed);
    MINING_ERROR_COUNT.fetch_add(2, std::sync::atomic::Ordering::Relaxed);
    PRUNE_ERROR_COUNT.fetch_add(3, std::sync::atomic::Ordering::Relaxed);
    let view = super::get_ops_status();
    assert!(view.mining_error_count >= before_mining.saturating_add(2));
    assert!(view.prune_error_count >= before_prune.saturating_add(3));
}

#[test]
fn health_and_ops_status_expose_block_gas_limit() {
    init_stable_state();
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.block_gas_limit = 9_000_000;
        chain_state.query_instruction_soft_limit = 123_456;
        chain_state.update_instruction_soft_limit = 654_321;
        state.chain_state.set(chain_state);
    });
    let health = super::health();
    assert_eq!(health.block_gas_limit, 9_000_000);
    assert_eq!(health.query_instruction_soft_limit, 123_456);
    assert_eq!(health.update_instruction_soft_limit, 654_321);
    let ops = super::get_ops_status();
    assert_eq!(ops.block_gas_limit, 9_000_000);
    assert_eq!(ops.query_instruction_soft_limit, 123_456);
    assert_eq!(ops.update_instruction_soft_limit, 654_321);
}

#[test]
fn init_args_apply_instruction_soft_limits_only_when_present() {
    init_stable_state();
    let defaults = evm_db::stable_state::with_state(|state| *state.chain_state.get());
    let args = InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: vec![0x11u8; 20],
            amount: 1,
        }],
        wrap_canister_id: Principal::self_authenticating(b"wrap"),
        wrap_factory_address: vec![0x22u8; 20],
        wrap_config: None,
        query_instruction_soft_limit: Some(123),
        update_instruction_soft_limit: None,
    };

    super::apply_instruction_soft_limits_from_init_args(&args);

    evm_db::stable_state::with_state(|state| {
        let chain_state = *state.chain_state.get();
        assert_eq!(chain_state.query_instruction_soft_limit, 123);
        assert_eq!(
            chain_state.update_instruction_soft_limit,
            defaults.update_instruction_soft_limit
        );
    });
}

#[test]
fn init_args_can_override_both_instruction_soft_limits() {
    init_stable_state();
    let args = InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: vec![0x33u8; 20],
            amount: 1,
        }],
        wrap_canister_id: Principal::self_authenticating(b"wrap-2"),
        wrap_factory_address: vec![0x44u8; 20],
        wrap_config: None,
        query_instruction_soft_limit: Some(321),
        update_instruction_soft_limit: Some(654),
    };

    super::apply_instruction_soft_limits_from_init_args(&args);

    let health = super::health();
    assert_eq!(health.query_instruction_soft_limit, 321);
    assert_eq!(health.update_instruction_soft_limit, 654);
    let ops = super::get_ops_status();
    assert_eq!(ops.query_instruction_soft_limit, 321);
    assert_eq!(ops.update_instruction_soft_limit, 654);
}

#[test]
fn init_args_reject_zero_instruction_soft_limits() {
    let base = InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: vec![0x33u8; 20],
            amount: 1,
        }],
        wrap_canister_id: Principal::self_authenticating(b"wrap-zero"),
        wrap_factory_address: vec![0x44u8; 20],
        wrap_config: None,
        query_instruction_soft_limit: None,
        update_instruction_soft_limit: None,
    };
    let mut query_zero = base.clone();
    query_zero.query_instruction_soft_limit = Some(0);
    assert_eq!(
        query_zero.validate().expect_err("query zero"),
        "query_instruction_soft_limit must be > 0"
    );

    let mut update_zero = base;
    update_zero.update_instruction_soft_limit = Some(0);
    assert_eq!(
        update_zero.validate().expect_err("update zero"),
        "update_instruction_soft_limit must be > 0"
    );
}

#[test]
fn init_args_apply_wrap_config_to_integrated_state() {
    init_stable_state();
    let fee_ledger = Principal::self_authenticating(b"fee-ledger");
    let native_ledger = Principal::self_authenticating(b"native-ledger");
    let allowed_asset = Principal::self_authenticating(b"asset-ledger");
    let args = InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: vec![0x33u8; 20],
            amount: 1,
        }],
        wrap_canister_id: Principal::self_authenticating(b"wrap-integrated"),
        wrap_factory_address: vec![0x44u8; 20],
        wrap_config: Some(WrapConfigArgs {
            fee_ledger_canister: fee_ledger,
            native_ledger_canister: native_ledger,
            cycle_fee_e8s: 1_000_000,
            gas_price_buffer_bps: 12_000,
            allowed_assets: vec![allowed_asset],
        }),
        query_instruction_soft_limit: None,
        update_instruction_soft_limit: None,
    };

    super::apply_wrap_config_from_init_args(&args);

    evm_db::stable_state::with_state(|state| {
        let fee_policy = state.wrap_fee_policy.get();
        assert_eq!(fee_policy.fee_ledger_canister, fee_ledger.as_slice());
        assert_eq!(fee_policy.cycle_fee_e8s, 1_000_000);
        assert_eq!(fee_policy.gas_price_buffer_bps, 12_000);
        assert_eq!(
            state.wrap_evm_config.get().wrap_factory_address,
            vec![0x44u8; 20]
        );
        assert_eq!(
            state.wrap_native_ledger_canister.get(),
            native_ledger.as_slice()
        );
        assert_eq!(
            state
                .wrap_allowed_assets
                .get(&allowed_asset.as_slice().to_vec()),
            Some(1)
        );
    });
}

#[test]
fn init_args_reject_invalid_wrap_config() {
    let base = InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: vec![0x33u8; 20],
            amount: 1,
        }],
        wrap_canister_id: Principal::self_authenticating(b"wrap-integrated"),
        wrap_factory_address: vec![0x44u8; 20],
        wrap_config: Some(WrapConfigArgs {
            fee_ledger_canister: Principal::anonymous(),
            native_ledger_canister: Principal::self_authenticating(b"native-ledger"),
            cycle_fee_e8s: 1_000_000,
            gas_price_buffer_bps: 12_000,
            allowed_assets: vec![Principal::self_authenticating(b"asset-ledger")],
        }),
        query_instruction_soft_limit: None,
        update_instruction_soft_limit: None,
    };
    assert_eq!(
        base.validate().expect_err("anonymous fee ledger"),
        "wrap.fee_ledger_anonymous"
    );

    let mut bad_gas = base.clone();
    let config = bad_gas.wrap_config.as_mut().expect("config");
    config.fee_ledger_canister = Principal::self_authenticating(b"fee-ledger");
    config.gas_price_buffer_bps = 9_999;
    assert_eq!(
        bad_gas.validate().expect_err("bad gas buffer"),
        "wrap.gas_price_buffer_bps_out_of_range"
    );

    let mut native_asset = bad_gas;
    let config = native_asset.wrap_config.as_mut().expect("config");
    config.gas_price_buffer_bps = 12_000;
    config.allowed_assets = vec![config.native_ledger_canister];
    assert_eq!(
        native_asset.validate().expect_err("native asset"),
        "wrap.native_ledger_not_wrappable"
    );
}

#[test]
fn quote_wrap_request_rejects_disallowed_asset_before_gas_quote() {
    init_stable_state();
    let fee_ledger = Principal::self_authenticating(b"fee-ledger");
    let native_ledger = Principal::self_authenticating(b"native-ledger");
    let allowed_asset = Principal::self_authenticating(b"asset-ledger");
    let args = InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: vec![0x33u8; 20],
            amount: 1,
        }],
        wrap_canister_id: Principal::self_authenticating(b"wrap-integrated"),
        wrap_factory_address: vec![0x44u8; 20],
        wrap_config: Some(WrapConfigArgs {
            fee_ledger_canister: fee_ledger,
            native_ledger_canister: native_ledger,
            cycle_fee_e8s: 1_000_000,
            gas_price_buffer_bps: 12_000,
            allowed_assets: vec![allowed_asset],
        }),
        query_instruction_soft_limit: None,
        update_instruction_soft_limit: None,
    };
    super::apply_wrap_config_from_init_args(&args);

    let out = super::quote_wrap_request(QuoteWrapRequestArgs {
        asset_id: Principal::self_authenticating(b"other-ledger"),
        amount_e8s: Nat::from(1u128),
        evm_recipient: vec![0x55; 20],
        gas_limit: 21_000,
    });

    match out {
        Err(ApiError::Rejected(detail)) => assert_eq!(detail.code, "asset.not_allowed"),
        other => panic!("unexpected quote result: {other:?}"),
    }
}

#[test]
fn quote_wrap_request_allowed_asset_uses_floor_when_fee_sample_missing() {
    init_stable_state();
    let fee_ledger = Principal::self_authenticating(b"fee-ledger");
    let native_ledger = Principal::self_authenticating(b"native-ledger");
    let allowed_asset = Principal::self_authenticating(b"asset-ledger");
    let args = InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: vec![0x33u8; 20],
            amount: 1,
        }],
        wrap_canister_id: Principal::self_authenticating(b"wrap-integrated"),
        wrap_factory_address: vec![0x44u8; 20],
        wrap_config: Some(WrapConfigArgs {
            fee_ledger_canister: fee_ledger,
            native_ledger_canister: native_ledger,
            cycle_fee_e8s: 1_000_000,
            gas_price_buffer_bps: 12_000,
            allowed_assets: vec![allowed_asset],
        }),
        query_instruction_soft_limit: None,
        update_instruction_soft_limit: None,
    };
    super::apply_wrap_config_from_init_args(&args);

    let out = super::quote_wrap_request(QuoteWrapRequestArgs {
        asset_id: allowed_asset,
        amount_e8s: Nat::from(1u128),
        evm_recipient: vec![0x55; 20],
        gas_limit: 21_000,
    })
    .expect("quote should use floor without fee sample");

    assert_eq!(out.charged_gas_price_wei, Nat::from(180_000_000_000u128));
    assert_eq!(out.charged_fee_e8s, Nat::from(1_378_000u128));
    assert_eq!(out.cycle_fee_e8s, 1_000_000);
    assert_eq!(out.fee_ledger_canister, fee_ledger);
}

#[test]
fn quote_native_deposit_uses_integrated_fee_policy() {
    init_stable_state();
    let fee_ledger = Principal::self_authenticating(b"fee-ledger");
    let native_ledger = Principal::self_authenticating(b"native-ledger");
    let allowed_asset = Principal::self_authenticating(b"asset-ledger");
    let args = InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: vec![0x33u8; 20],
            amount: 1,
        }],
        wrap_canister_id: Principal::self_authenticating(b"wrap-integrated"),
        wrap_factory_address: vec![0x44u8; 20],
        wrap_config: Some(WrapConfigArgs {
            fee_ledger_canister: fee_ledger,
            native_ledger_canister: native_ledger,
            cycle_fee_e8s: 1_000_000,
            gas_price_buffer_bps: 12_000,
            allowed_assets: vec![allowed_asset],
        }),
        query_instruction_soft_limit: None,
        update_instruction_soft_limit: None,
    };
    super::apply_wrap_config_from_init_args(&args);

    let out = super::quote_native_deposit(QuoteNativeDepositArgs {
        amount_e8s: Nat::from(1u128),
        evm_recipient: vec![0x55; 20],
    })
    .expect("quote");

    assert_eq!(out.charged_fee_e8s, Nat::from(1_000_000u64));
    assert_eq!(out.fee_ledger_canister, fee_ledger);
    assert_eq!(out.native_ledger_canister, native_ledger);
}

#[test]
fn native_deposit_funding_validation_stops_before_pending_when_native_unconfigured() {
    init_stable_state();
    let fee_ledger = Principal::self_authenticating(b"fee-ledger");
    with_state_mut(|state| {
        state
            .wrap_fee_policy
            .set(evm_db::chain_data::FeePolicyStored {
                fee_ledger_canister: fee_ledger.as_slice().to_vec(),
                cycle_fee_e8s: 1,
                gas_price_buffer_bps: 12_000,
            });
    });

    let out = super::prepare_native_deposit_funding(fee_ledger, 1);

    match out {
        Err(ApiError::Internal(detail)) => assert_eq!(detail.code, "wrap_config.unconfigured"),
        other => panic!("unexpected native deposit funding result: {other:?}"),
    }
    with_state(|state| {
        assert_eq!(state.wrap_pending_submissions.len(), 0);
    });
}

#[test]
fn wrap_request_ids_match_legacy_domain_separators() {
    let wrap_id =
        super::derive_wrap_request_id(&[1, 2, 3], &[4, 5], &[0; 32], &[0x55; 20], 7, 21_000);
    assert_eq!(
        wrap_id,
        [
            0x98, 0xa8, 0x8f, 0x98, 0xe3, 0xc0, 0xe5, 0x65, 0xf2, 0x71, 0x1b, 0x8f, 0x5c, 0x26,
            0xca, 0x4a, 0xf5, 0xab, 0x7d, 0xbf, 0x61, 0x3e, 0xa2, 0xbe, 0x6a, 0x08, 0x53, 0x0e,
            0x86, 0xb2, 0xd9, 0x57,
        ]
    );

    let native_id = super::derive_native_deposit_request_id(&[1, 2, 3], &[9; 32]);
    assert_eq!(
        native_id,
        [
            0xa5, 0x99, 0x10, 0x1a, 0xd7, 0x10, 0xd2, 0x7b, 0x51, 0xfd, 0xf1, 0xd6, 0x01, 0xae,
            0x34, 0xa5, 0xa3, 0x16, 0xb3, 0x54, 0x34, 0x37, 0xef, 0x60, 0x01, 0x42, 0xcd, 0x64,
            0x1d, 0x7d, 0xff, 0x23,
        ]
    );
}

#[test]
fn factory_mint_for_asset_call_data_matches_abi_layout() {
    let asset = Principal::self_authenticating(b"asset-ledger");
    let recipient = vec![0x55; 20];
    let amount = vec![0xabu8; 32];

    let data = super::encode_factory_mint_for_asset_call_data(
        asset.as_slice(),
        8,
        recipient.as_slice(),
        amount.as_slice(),
    )
    .expect("call data");

    let asset_len = asset.as_slice().len();
    let padded_asset_len = asset_len.div_ceil(32) * 32;
    assert_eq!(data.len(), 4 + 32 * 5 + padded_asset_len);
    assert_eq!(&data[0..4], &super::factory_mint_for_asset_selector());
    assert_eq!(&data[4..36], &super::u256_from_u64(128));
    assert_eq!(&data[36..68], &super::u256_from_u64(8));
    assert_eq!(&data[68..80], &[0u8; 12]);
    assert_eq!(&data[80..100], recipient.as_slice());
    assert_eq!(&data[100..132], amount.as_slice());
    assert_eq!(&data[132..164], &super::u256_from_u64(asset_len as u64));
    assert_eq!(&data[164..164 + asset_len], asset.as_slice());
    assert!(data[164 + asset_len..].iter().all(|byte| *byte == 0));
}

#[test]
fn get_request_returns_wrap_overview_from_integrated_state() {
    init_stable_state();
    let request_id = TxId([0xabu8; 32]);
    with_state_mut(|state| {
        state.wrap_requests.insert(
            request_id,
            WrapStoredRequest {
                caller: vec![1],
                asset_id: vec![2],
                amount: vec![0; 32],
                evm_recipient: vec![3; 20],
                gas_limit: 21_000,
                fee_ledger_canister: vec![7],
                max_fee_e8s: 9,
                quoted_gas_price_wei: 10,
                fee_created_at_time: 1,
                pull_created_at_time: 2,
                withdraw_created_at_time: 0,
                result: WrapRequestResult {
                    status: RequestStatus::Succeeded,
                    pull_ledger_tx_id: Some(vec![4]),
                    mint_tx_id: Some(vec![5]),
                    error_code: None,
                    withdrawn: false,
                    withdraw_ledger_tx_id: None,
                    withdraw_error_code: None,
                    withdraw_in_progress: false,
                    mint_failed_recoverable: false,
                    fee_ledger_tx_id: Some(vec![6]),
                    charged_fee_e8s: Some(7),
                    charged_gas_price_wei: Some(8),
                    stage: WrapRequestStage::Succeeded,
                    updated_at: 3,
                    mint_nonce: None,
                    mint_submitted_at_time: 0,
                    mint_submit_status: MintSubmitStatus::NotSubmitted,
                },
            },
        );
    });

    let overview = super::get_request(request_id.0.to_vec()).expect("overview");
    assert_eq!(overview.kind, super::RequestKind::Wrap);
    assert_eq!(overview.status, RequestStatus::Succeeded);
    assert_eq!(overview.pull_ledger_tx_id, Some(vec![4]));
    assert_eq!(overview.mint_tx_id, Some(vec![5]));
    assert_eq!(overview.fee_ledger_tx_id, Some(vec![6]));
    assert_eq!(overview.charged_fee_e8s, Some(Nat::from(7u64)));
    assert_eq!(overview.charged_gas_price_wei, Some(Nat::from(8u64)));
}

#[test]
fn existing_native_deposit_response_is_idempotent_for_same_request() {
    init_stable_state();
    let caller = Principal::self_authenticating(b"native-deposit-caller");
    let request_id = TxId([0xbcu8; 32]);
    with_state_mut(|state| {
        state.wrap_requests.insert(
            request_id,
            WrapStoredRequest {
                caller: caller.as_slice().to_vec(),
                asset_id: vec![2],
                amount: vec![0x11; 32],
                evm_recipient: vec![0x55; 20],
                gas_limit: 0,
                fee_ledger_canister: vec![7],
                max_fee_e8s: 9,
                quoted_gas_price_wei: 0,
                fee_created_at_time: 1,
                pull_created_at_time: 2,
                withdraw_created_at_time: 0,
                result: WrapRequestResult {
                    status: RequestStatus::Succeeded,
                    pull_ledger_tx_id: Some(vec![4]),
                    mint_tx_id: Some(request_id.0.to_vec()),
                    error_code: None,
                    withdrawn: false,
                    withdraw_ledger_tx_id: None,
                    withdraw_error_code: None,
                    withdraw_in_progress: false,
                    mint_failed_recoverable: false,
                    fee_ledger_tx_id: Some(vec![6]),
                    charged_fee_e8s: Some(7),
                    charged_gas_price_wei: Some(0),
                    stage: WrapRequestStage::Succeeded,
                    updated_at: 3,
                    mint_nonce: None,
                    mint_submitted_at_time: 0,
                    mint_submit_status: MintSubmitStatus::NotSubmitted,
                },
            },
        );
    });

    let out = super::existing_native_deposit_response(request_id, [0x11; 32], &[0x55; 20], caller)
        .expect("existing")
        .expect("ok");
    assert_eq!(out.request_id, request_id.0.to_vec());
    assert_eq!(out.charged_fee_e8s, Nat::from(7u64));
    assert_eq!(out.fee_ledger_tx_id, vec![6]);
}

#[test]
fn existing_native_deposit_response_rejects_idempotency_mismatch() {
    init_stable_state();
    let caller = Principal::self_authenticating(b"native-deposit-caller");
    let request_id = TxId([0xbdu8; 32]);
    with_state_mut(|state| {
        state.wrap_requests.insert(
            request_id,
            WrapStoredRequest {
                caller: caller.as_slice().to_vec(),
                asset_id: vec![2],
                amount: vec![0x11; 32],
                evm_recipient: vec![0x55; 20],
                gas_limit: 0,
                fee_ledger_canister: vec![7],
                max_fee_e8s: 9,
                quoted_gas_price_wei: 0,
                fee_created_at_time: 1,
                pull_created_at_time: 2,
                withdraw_created_at_time: 0,
                result: WrapRequestResult {
                    status: RequestStatus::Succeeded,
                    pull_ledger_tx_id: Some(vec![4]),
                    mint_tx_id: Some(request_id.0.to_vec()),
                    error_code: None,
                    withdrawn: false,
                    withdraw_ledger_tx_id: None,
                    withdraw_error_code: None,
                    withdraw_in_progress: false,
                    mint_failed_recoverable: false,
                    fee_ledger_tx_id: Some(vec![6]),
                    charged_fee_e8s: Some(7),
                    charged_gas_price_wei: Some(0),
                    stage: WrapRequestStage::Succeeded,
                    updated_at: 3,
                    mint_nonce: None,
                    mint_submitted_at_time: 0,
                    mint_submit_status: MintSubmitStatus::NotSubmitted,
                },
            },
        );
    });

    let out = super::existing_native_deposit_response(request_id, [0x22; 32], &[0x55; 20], caller)
        .expect("existing");
    match out {
        Err(ApiError::Rejected(detail)) => {
            assert_eq!(detail.code, "request.idempotency_mismatch")
        }
        other => panic!("unexpected existing response: {other:?}"),
    }
}

#[test]
fn ensure_wrap_request_before_fee_saves_quote_then_enqueues_after_fee() {
    init_stable_state();
    let caller = Principal::self_authenticating(b"wrap-caller");
    let request_id = TxId([0xacu8; 32]);
    let args = super::NormalizedSubmitWrapRequest {
        request_id,
        asset_id: vec![1],
        amount: vec![0x11; 32],
        evm_recipient: vec![0x55; 20],
        gas_limit: 21_000,
        max_fee_e8s: 100,
        quoted_gas_price_wei: 200,
        fee_ledger_canister: Principal::self_authenticating(b"fee-ledger"),
    };

    super::ensure_wrap_request_before_fee(args, caller, 7, 8).expect("insert");

    with_state(|state| {
        let req = state.wrap_requests.get(&request_id).expect("request");
        assert_eq!(req.caller, caller.as_slice());
        assert_eq!(req.result.status, RequestStatus::Queued);
        assert_eq!(req.result.stage, WrapRequestStage::FeePending);
        assert_eq!(req.result.fee_ledger_tx_id, None);
        assert_eq!(req.result.charged_fee_e8s, Some(7));
        assert_eq!(req.result.charged_gas_price_wei, Some(8));
        assert_eq!(state.wrap_queue.get(&0), None);
    });

    super::record_wrap_fee_collected(request_id, vec![9]).expect("fee");
    super::enqueue_wrap_request_once(request_id);

    with_state(|state| {
        let req = state.wrap_requests.get(&request_id).expect("request");
        assert_eq!(req.caller, caller.as_slice());
        assert_eq!(req.result.status, RequestStatus::Queued);
        assert_eq!(req.result.stage, WrapRequestStage::FeeCollected);
        assert_eq!(req.result.fee_ledger_tx_id, Some(vec![9]));
        assert_eq!(req.result.charged_fee_e8s, Some(7));
        assert_eq!(req.result.charged_gas_price_wei, Some(8));
        assert_eq!(state.wrap_queue.get(&0), Some(request_id));
    });
}

#[test]
fn existing_wrap_request_response_is_idempotent_for_same_request() {
    init_stable_state();
    let caller = Principal::self_authenticating(b"wrap-caller");
    let fee_ledger = Principal::self_authenticating(b"fee-ledger");
    let request_id = TxId([0xadu8; 32]);
    let args = super::NormalizedSubmitWrapRequest {
        request_id,
        asset_id: vec![1],
        amount: vec![0x11; 32],
        evm_recipient: vec![0x55; 20],
        gas_limit: 21_000,
        max_fee_e8s: 100,
        quoted_gas_price_wei: 200,
        fee_ledger_canister: fee_ledger,
    };
    super::ensure_wrap_request_before_fee(args, caller, 7, 8).expect("insert");
    super::record_wrap_fee_collected(request_id, vec![9]).expect("fee");
    let retry_args = super::NormalizedSubmitWrapRequest {
        request_id,
        asset_id: vec![1],
        amount: vec![0x11; 32],
        evm_recipient: vec![0x55; 20],
        gas_limit: 21_000,
        max_fee_e8s: 100,
        quoted_gas_price_wei: 200,
        fee_ledger_canister: fee_ledger,
    };

    let out = super::existing_wrap_request_response(&retry_args, caller)
        .expect("existing")
        .expect("ok");

    assert_eq!(out.request_id, request_id.0.to_vec());
    assert_eq!(out.charged_fee_e8s, Nat::from(7u64));
    assert_eq!(out.charged_gas_price_wei, Nat::from(8u64));
    assert_eq!(out.fee_ledger_tx_id, vec![9]);
}

#[test]
fn existing_wrap_request_response_rejects_idempotency_mismatch() {
    init_stable_state();
    let caller = Principal::self_authenticating(b"wrap-caller");
    let fee_ledger = Principal::self_authenticating(b"fee-ledger");
    let request_id = TxId([0xaeu8; 32]);
    let args = super::NormalizedSubmitWrapRequest {
        request_id,
        asset_id: vec![1],
        amount: vec![0x11; 32],
        evm_recipient: vec![0x55; 20],
        gas_limit: 21_000,
        max_fee_e8s: 100,
        quoted_gas_price_wei: 200,
        fee_ledger_canister: fee_ledger,
    };
    super::ensure_wrap_request_before_fee(args, caller, 7, 8).expect("insert");
    super::record_wrap_fee_collected(request_id, vec![9]).expect("fee");
    let retry_args = super::NormalizedSubmitWrapRequest {
        request_id,
        asset_id: vec![1],
        amount: vec![0x22; 32],
        evm_recipient: vec![0x55; 20],
        gas_limit: 21_000,
        max_fee_e8s: 100,
        quoted_gas_price_wei: 200,
        fee_ledger_canister: fee_ledger,
    };

    let out = super::existing_wrap_request_response(&retry_args, caller).expect("existing");

    match out {
        Err(ApiError::Rejected(detail)) => {
            assert_eq!(detail.code, "request.idempotency_mismatch")
        }
        other => panic!("unexpected existing response: {other:?}"),
    }
}

#[test]
fn require_wrap_canister_caller_rejects_non_wrap_caller() {
    init_stable_state();
    let wrap = Principal::self_authenticating(b"wrap-integrated");
    install_runtime_wrap_canister_id(wrap);

    let err = super::require_wrap_canister_caller(Principal::self_authenticating(b"attacker"))
        .expect_err("non-wrap caller");

    assert_eq!(err, "auth.wrap_canister_required");
    assert!(super::require_wrap_canister_caller(wrap).is_ok());
}

#[test]
fn get_request_returns_unwrap_dispatch_overview_from_integrated_state() {
    init_stable_state();
    let request_id = TxId([0xcdu8; 32]);
    with_state_mut(|state| {
        state.unwrap_requests.insert(
            request_id,
            UnwrapDispatchRequest {
                asset_id: vec![1],
                amount: [0; 32],
                recipient: vec![2],
                status: UnwrapRequestStatus::DispatchFailed,
                ledger_tx_id: None,
                error_code: Some("dispatch.failed".to_string()),
                updated_at: 1,
                transfer_created_at_time: 0,
            },
        );
    });

    let overview = super::get_request(request_id.0.to_vec()).expect("overview");
    assert_eq!(overview.kind, super::RequestKind::Unwrap);
    assert_eq!(overview.status, RequestStatus::Failed);
    assert_eq!(
        overview.dispatch_status,
        Some(super::RequestDispatchStatusView::DispatchFailed)
    );
    assert_eq!(overview.dispatch_error, Some("dispatch.failed".to_string()));
}

#[test]
fn insert_unwrap_dispatch_request_enqueues_integrated_request() {
    init_stable_state();
    let request_id = TxId([0xee; 32]);

    super::insert_unwrap_dispatch_request(request_id, vec![1, 2], [0x11; 32], vec![3, 4])
        .expect("insert");

    with_state(|state| {
        let req = state.unwrap_requests.get(&request_id).expect("request");
        assert_eq!(req.asset_id, vec![1, 2]);
        assert_eq!(req.amount, [0x11; 32]);
        assert_eq!(req.recipient, vec![3, 4]);
        assert_eq!(req.status, UnwrapRequestStatus::Queued);
        assert_eq!(state.unwrap_dispatch_queue.get(&0), Some(request_id));
    });
}

#[test]
fn insert_unwrap_dispatch_request_rejects_idempotency_mismatch() {
    init_stable_state();
    let request_id = TxId([0xef; 32]);
    super::insert_unwrap_dispatch_request(request_id, vec![1], [0x11; 32], vec![3])
        .expect("insert");

    let err = super::insert_unwrap_dispatch_request(request_id, vec![2], [0x11; 32], vec![3])
        .expect_err("mismatch");
    assert_eq!(err, "request.idempotency_mismatch");
}

#[test]
fn retry_unwrap_dispatch_requeues_failed_request() {
    init_stable_state();
    let request_id = TxId([0xf1; 32]);
    with_state_mut(|state| {
        state.unwrap_requests.insert(
            request_id,
            UnwrapDispatchRequest {
                asset_id: vec![1],
                amount: [0x11; 32],
                recipient: vec![2],
                status: UnwrapRequestStatus::DispatchFailed,
                ledger_tx_id: None,
                error_code: Some("dispatch.failed".to_string()),
                updated_at: 1,
                transfer_created_at_time: 0,
            },
        );
    });

    let overview = super::retry_unwrap_dispatch(request_id.0.to_vec()).expect("retry");
    assert_eq!(overview.status, RequestStatus::Queued);
    assert_eq!(
        overview.dispatch_status,
        Some(super::RequestDispatchStatusView::Queued)
    );
    with_state(|state| {
        let req = state.unwrap_requests.get(&request_id).expect("request");
        assert_eq!(req.status, UnwrapRequestStatus::Queued);
        assert_eq!(req.error_code, None);
        assert_eq!(state.unwrap_dispatch_queue.get(&0), Some(request_id));
    });
}

#[test]
fn internal_unwrap_dispatch_fails_invalid_integrated_ledger_without_call() {
    init_stable_state();
    let request_id = TxId([0xf2; 32]);
    let req = UnwrapDispatchRequest {
        asset_id: Vec::new(),
        amount: [0x11; 32],
        recipient: Principal::self_authenticating(b"recipient")
            .as_slice()
            .to_vec(),
        status: UnwrapRequestStatus::Dispatching,
        ledger_tx_id: None,
        error_code: None,
        updated_at: 1,
        transfer_created_at_time: 1,
    };

    let out = run_ready_future(super::dispatch_unwrap_request_internal(request_id, req));

    assert_eq!(out.status, UnwrapRequestStatus::DispatchFailed);
    assert_eq!(out.ledger_tx_id, None);
    assert_eq!(out.error_code, Some("wrap_config.unconfigured".to_string()));
}

#[test]
fn native_withdraw_marker_uses_gross_semantics() {
    let mut request = sample_unwrap_request(UnwrapRequestStatus::Queued, None, 1);
    request.asset_id = super::NATIVE_WITHDRAW_ASSET_MARKER.to_vec();

    assert!(super::is_native_withdraw_dispatch_request(&request));
}

#[test]
fn native_withdraw_gross_transfer_amount_subtracts_fee() {
    let amount = Nat::from(100_000u128);

    let out = super::native_withdraw_gross_transfer_amount(&amount, 10_000).expect("amount");

    assert_eq!(out, Nat::from(90_000u128));
    assert_eq!(
        super::native_withdraw_gross_transfer_amount(&amount, 100_000).expect_err("fee"),
        "native_withdraw.amount_not_above_fee"
    );
}

#[test]
fn apply_post_upgrade_migrations_resyncs_gas_limit_and_fee_floors_only() {
    init_stable_state();
    let current = evm_db::meta::current_schema_version();
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 333_000_000_000;
        chain_state.min_gas_price = 111_000_000_000;
        chain_state.min_priority_fee = 222_000_000_000;
        chain_state.block_gas_limit = 6_000_000;
        state.chain_state.set(chain_state);
    });
    let mut meta = evm_db::meta::Meta::new();
    meta.schema_version = current.saturating_sub(1);
    meta.last_migration_from = meta.schema_version;
    meta.last_migration_to = meta.schema_version;
    evm_db::meta::set_meta(meta);

    super::apply_post_upgrade_migrations();
    // test では post-upgrade 後の migration 完了状態まで明示的に進めて確認する。
    run_post_upgrade_migrations_until_settled();

    evm_db::stable_state::with_state(|state| {
        let chain_state = *state.chain_state.get();
        assert_eq!(chain_state.block_gas_limit, DEFAULT_BLOCK_GAS_LIMIT);
        assert_eq!(chain_state.min_gas_price, DEFAULT_MIN_FEE_FLOOR);
        assert_eq!(chain_state.min_priority_fee, DEFAULT_MIN_FEE_FLOOR);
        assert_eq!(chain_state.base_fee, 333_000_000_000);
    });
    let health = super::health();
    assert_eq!(health.block_gas_limit, DEFAULT_BLOCK_GAS_LIMIT);
    let ops = super::get_ops_status();
    assert_eq!(ops.block_gas_limit, DEFAULT_BLOCK_GAS_LIMIT);
    let meta = evm_db::meta::get_meta();
    assert_eq!(meta.schema_version, current);
    assert!(!meta.needs_migration);
    assert_eq!(meta.last_migration_from, current - 1);
    assert_eq!(meta.last_migration_to, current);
}

#[test]
fn apply_post_upgrade_migrations_resyncs_any_stale_floor_values() {
    init_stable_state();
    let current = evm_db::meta::current_schema_version();
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 444_000_000_000;
        chain_state.min_gas_price = 999_000_000_000;
        chain_state.min_priority_fee = 888_000_000_000;
        chain_state.block_gas_limit = 9_000_000;
        state.chain_state.set(chain_state);
    });
    let mut meta = evm_db::meta::Meta::new();
    meta.schema_version = current.saturating_sub(1);
    meta.last_migration_from = meta.schema_version;
    meta.last_migration_to = meta.schema_version;
    evm_db::meta::set_meta(meta);

    super::apply_post_upgrade_migrations();
    // test では post-upgrade 後の migration 完了状態まで明示的に進めて確認する。
    run_post_upgrade_migrations_until_settled();

    evm_db::stable_state::with_state(|state| {
        let chain_state = *state.chain_state.get();
        assert_eq!(chain_state.block_gas_limit, DEFAULT_BLOCK_GAS_LIMIT);
        assert_eq!(chain_state.min_gas_price, DEFAULT_MIN_FEE_FLOOR);
        assert_eq!(chain_state.min_priority_fee, DEFAULT_MIN_FEE_FLOOR);
        assert_eq!(chain_state.base_fee, 444_000_000_000);
    });
}

#[test]
fn get_ops_status_reports_ops_config() {
    init_stable_state();
    let ops = super::get_ops_status();
    assert_eq!(ops.config.low_watermark, 2_000_000_000_000);
    assert_eq!(ops.config.critical, 1_000_000_000_000);
}

#[test]
fn set_prune_policy_rejects_non_positive_max_ops() {
    init_stable_state();
    let policy = PrunePolicyView {
        target_bytes: 1,
        retain_days: 1,
        retain_blocks: 1,
        headroom_ratio_bps: 2000,
        hard_emergency_ratio_bps: 9500,
        max_ops_per_tick: 0,
    };
    let err = validate_prune_policy_input(&policy).expect_err("max ops must be positive");
    assert_eq!(err, "input.prune.max_ops_per_tick.non_positive");
}

#[test]
fn should_prune_on_block_event_only_on_84_multiples() {
    assert!(!super::should_prune_on_block_event(0));
    assert!(!super::should_prune_on_block_event(83));
    assert!(super::should_prune_on_block_event(84));
    assert!(!super::should_prune_on_block_event(85));
    assert!(super::should_prune_on_block_event(168));
}

#[test]
fn meta_corruption_reflects_in_write_blocking_status() {
    init_stable_state();
    let mut meta = evm_db::meta::Meta::new();
    meta.needs_migration = true;
    set_meta(meta);
    let view = super::get_ops_status();
    assert!(view.needs_migration);
    assert_eq!(view.decode_failure_count, 0);
    assert_eq!(view.decode_failure_last_label, None);
    let reason = reject_write_reason().expect("write should be blocked");
    assert_eq!(reason, "ops.write.needs_migration");
}

#[test]
fn ops_status_needs_migration_matches_state_root_pending() {
    init_stable_state();
    let mut meta = evm_db::meta::Meta::new();
    meta.needs_migration = false;
    meta.schema_version = current_schema_version();
    set_meta(meta);
    // state_root_meta.initialized は初期値 false のため migration_pending() は true。
    let view = super::get_ops_status();
    assert!(view.needs_migration);
}

#[test]
fn get_block_returns_not_found_for_corrupt_block_payload() {
    init_stable_state();
    with_state_mut(|state| {
        let corrupt = BlockData::from_bytes(Cow::Owned(vec![0u8; 1]));
        let ptr = state
            .blob_store
            .store_bytes(corrupt.to_bytes().as_ref())
            .expect("store corrupt block");
        state.blocks.insert(9, ptr);
    });

    let out = super::get_block(9);
    assert!(matches!(out, Err(super::LookupError::NotFound)));
}

#[test]
fn get_block_returns_pruned_for_pruned_boundary() {
    init_stable_state();
    with_state_mut(|state| {
        let mut prune_state = *state.prune_state.get();
        prune_state.set_pruned_before(8);
        state.prune_state.set(prune_state);
    });

    let out = super::get_block(8);
    assert!(matches!(
        out,
        Err(super::LookupError::Pruned {
            pruned_before_block: 8
        })
    ));
}

#[test]
fn get_block_returns_ok_when_prune_boundary_is_absent() {
    init_stable_state();
    with_state_mut(|state| {
        let block = BlockData::new(
            0,
            [0x20; 32],
            [0x21; 32],
            123,
            1,
            DEFAULT_BLOCK_GAS_LIMIT,
            0,
            [0x22; 20],
            Vec::new(),
            [0x23; 32],
            [0x24; 32],
        );
        let ptr = state
            .blob_store
            .store_bytes(block.to_bytes().as_ref())
            .expect("store unpruned block");
        state.blocks.insert(0, ptr);
    });

    let out = super::get_block(0).expect("block without prune boundary");
    assert_eq!(out.number, 0);
}

#[test]
fn get_block_returns_ok_for_retained_block() {
    init_stable_state();
    with_state_mut(|state| {
        let block = BlockData::new(
            9,
            [0x10; 32],
            [0x11; 32],
            123,
            1,
            DEFAULT_BLOCK_GAS_LIMIT,
            0,
            [0x12; 20],
            Vec::new(),
            [0x13; 32],
            [0x14; 32],
        );
        let ptr = state
            .blob_store
            .store_bytes(block.to_bytes().as_ref())
            .expect("store retained block");
        state.blocks.insert(9, ptr);
        let mut prune_state = *state.prune_state.get();
        prune_state.set_pruned_before(5);
        state.prune_state.set(prune_state);
    });

    let out = super::get_block(9).expect("retained block");
    assert_eq!(out.number, 9);
}

#[test]
fn get_receipt_returns_not_found_for_corrupt_receipt_payload() {
    init_stable_state();
    let tx_id = TxId([0x81u8; 32]);
    with_state_mut(|state| {
        let corrupt = ReceiptLike::from_bytes(Cow::Owned(vec![0u8; 1]));
        let ptr = state
            .blob_store
            .store_bytes(corrupt.to_bytes().as_ref())
            .expect("store corrupt receipt");
        state.receipts.insert(tx_id, ptr);
    });

    let out = super::get_receipt(tx_id.0.to_vec());
    assert!(matches!(out, Err(super::LookupError::NotFound)));
}

#[test]
fn get_receipt_returns_ok_for_retained_receipt() {
    init_stable_state();
    let tx_id = TxId([0x82u8; 32]);
    with_state_mut(|state| {
        let receipt = fake_receipt_for_lookup(tx_id, 9);
        let ptr = state
            .blob_store
            .store_bytes(receipt.to_bytes().as_ref())
            .expect("store receipt");
        state.receipts.insert(tx_id, ptr);
        state.tx_locs.insert(tx_id, TxLoc::included(9, 0));
        let mut prune_state = *state.prune_state.get();
        prune_state.set_pruned_before(5);
        state.prune_state.set(prune_state);
    });

    let out = super::get_receipt(tx_id.0.to_vec()).expect("retained receipt");
    assert_eq!(out.tx_id, tx_id.0.to_vec());
    assert_eq!(out.block_number, 9);
}

#[test]
fn get_receipt_returns_pruned_when_location_is_before_boundary() {
    init_stable_state();
    let tx_id = TxId([0x83u8; 32]);
    with_state_mut(|state| {
        state.tx_locs.insert(tx_id, TxLoc::included(4, 0));
        let mut prune_state = *state.prune_state.get();
        prune_state.set_pruned_before(5);
        state.prune_state.set(prune_state);
    });

    let out = super::get_receipt(tx_id.0.to_vec());
    assert!(matches!(
        out,
        Err(super::LookupError::Pruned {
            pruned_before_block: 5
        })
    ));
}

fn fake_receipt_for_lookup(tx_id: TxId, block_number: u64) -> ReceiptLike {
    ReceiptLike {
        tx_id,
        block_number,
        tx_index: 0,
        status: 1,
        gas_used: 0,
        effective_gas_price: 0,
        l1_data_fee: 0,
        operator_fee: 0,
        total_fee: 0,
        return_data_hash: [0u8; 32],
        return_data: Vec::new(),
        contract_address: None,
        logs: Vec::new(),
    }
}

#[test]
fn memory_breakdown_reports_consistent_totals_and_known_regions() {
    init_stable_state();
    let view = super::memory_breakdown();
    assert_eq!(
        view.stable_bytes_total,
        view.stable_pages_total.saturating_mul(WASM_PAGE_SIZE_BYTES)
    );
    assert_eq!(
        view.heap_bytes,
        view.heap_pages.saturating_mul(WASM_PAGE_SIZE_BYTES)
    );
    assert_eq!(
        view.regions_bytes_total,
        view.regions_pages_total
            .saturating_mul(WASM_PAGE_SIZE_BYTES)
    );
    assert_eq!(
        view.unattributed_stable_bytes,
        view.unattributed_stable_pages
            .saturating_mul(WASM_PAGE_SIZE_BYTES)
    );
    assert_eq!(
        view.unattributed_stable_pages,
        view.stable_pages_total
            .saturating_sub(view.regions_pages_total)
    );
    assert_eq!(
        view.unattributed_stable_bytes,
        view.stable_bytes_total
            .saturating_sub(view.regions_bytes_total)
    );
    for pair in view.regions.windows(2) {
        assert!(pair[0].id <= pair[1].id, "regions must be sorted by id");
    }
    let mut has_tx_store = false;
    let mut has_blob_arena = false;
    let mut sum_pages = 0u64;
    let mut sum_bytes = 0u64;
    for region in &view.regions {
        assert_eq!(
            region.bytes,
            region.pages.saturating_mul(WASM_PAGE_SIZE_BYTES)
        );
        sum_pages = sum_pages.saturating_add(region.pages);
        sum_bytes = sum_bytes.saturating_add(region.bytes);
        if region.name == "TxStore" {
            has_tx_store = true;
        }
        if region.name == "BlobArena" {
            has_blob_arena = true;
        }
    }
    assert_eq!(sum_pages, view.regions_pages_total);
    assert_eq!(sum_bytes, view.regions_bytes_total);
    assert!(has_tx_store, "TxStore region must exist");
    assert!(has_blob_arena, "BlobArena region must exist");
}

#[test]
fn decode_failure_label_view_prefers_ascii_machine_code() {
    let mut raw = [0u8; 32];
    raw[..12].copy_from_slice(b"block_data_1");
    let out = decode_failure_label_view(raw);
    assert_eq!(out, Some("block_data_1".to_string()));
}

#[test]
fn decode_failure_label_view_falls_back_to_hex() {
    let mut raw = [0u8; 32];
    raw[0] = 0xff;
    raw[1] = 0x01;
    let out = decode_failure_label_view(raw).expect("hex label");
    assert!(out.starts_with("hex:"));
}

#[test]
fn rpc_eth_get_logs_paged_rejects_invalid_inputs() {
    init_stable_state();
    let cases = [
        (
            "reverse_range",
            EthLogFilterView {
                from_block: Some(10),
                to_block: Some(9),
                address: None,
                topic0: None,
                topic1: None,
                limit: Some(10),
            },
            10,
            GetLogsErrorView::InvalidArgument("from_block must be <= to_block".to_string()),
        ),
        (
            "range_too_large_with_filter_limit",
            EthLogFilterView {
                from_block: Some(0),
                to_block: Some(6_001),
                address: None,
                topic0: None,
                topic1: None,
                limit: Some(10),
            },
            0,
            GetLogsErrorView::RangeTooLarge,
        ),
        (
            "unsupported_topic1",
            EthLogFilterView {
                from_block: Some(0),
                to_block: Some(0),
                address: None,
                topic0: None,
                topic1: Some(vec![0u8; 32]),
                limit: Some(10),
            },
            10,
            GetLogsErrorView::UnsupportedFilter("topic1 is not supported".to_string()),
        ),
        (
            "over_limit_with_filter_limit",
            EthLogFilterView {
                from_block: Some(0),
                to_block: Some(0),
                address: None,
                topic0: None,
                topic1: None,
                limit: Some(2_001),
            },
            0,
            GetLogsErrorView::TooManyResults,
        ),
        (
            "range_too_large",
            EthLogFilterView {
                from_block: Some(0),
                to_block: Some(1_500),
                address: None,
                topic0: None,
                topic1: None,
                limit: None,
            },
            100,
            GetLogsErrorView::RangeTooLarge,
        ),
        (
            "over_limit",
            EthLogFilterView {
                from_block: Some(0),
                to_block: Some(1),
                address: None,
                topic0: None,
                topic1: None,
                limit: None,
            },
            5_000,
            GetLogsErrorView::TooManyResults,
        ),
    ];
    for (case, filter, page_limit, expected) in cases {
        let err = super::rpc_eth_get_logs_paged(filter, None, page_limit).expect_err(case);
        assert_eq!(err, expected, "{case}");
    }
}

#[test]
fn rpc_eth_get_logs_paged_normalizes_zero_limits_to_one() {
    init_stable_state();
    let cases = [
        (
            "zero_page_limit",
            EthLogFilterView {
                from_block: Some(0),
                to_block: Some(0),
                address: None,
                topic0: None,
                topic1: None,
                limit: None,
            },
            0,
        ),
        (
            "zero_filter_limit",
            EthLogFilterView {
                from_block: Some(0),
                to_block: Some(0),
                address: None,
                topic0: None,
                topic1: None,
                limit: Some(0),
            },
            0,
        ),
    ];
    for (case, filter, page_limit) in cases {
        let page = super::rpc_eth_get_logs_paged(filter, None, page_limit).expect(case);
        assert!(page.items.is_empty(), "{case}");
        assert!(page.next_cursor.is_none(), "{case}");
    }
}

#[test]
fn rpc_eth_get_storage_at_returns_zero_for_missing_slot() {
    init_stable_state();
    let out =
        super::rpc_eth_get_storage_at(vec![0u8; 20], vec![0u8; 32], super::RpcBlockTagView::Latest)
            .expect("storage");
    assert_eq!(out, vec![0u8; 32]);
}

#[test]
fn rpc_eth_get_storage_at_reads_existing_slot() {
    init_stable_state();
    let addr = [0x11u8; 20];
    let slot = [0x22u8; 32];
    evm_db::stable_state::with_state_mut(|state| {
        state
            .storage
            .insert(make_storage_key(addr, slot), U256Val([0x33u8; 32]));
    });
    let out =
        super::rpc_eth_get_storage_at(addr.to_vec(), slot.to_vec(), super::RpcBlockTagView::Latest)
            .expect("storage");
    assert_eq!(out, vec![0x33u8; 32]);
}

#[test]
fn rpc_eth_get_storage_at_rejects_bad_length() {
    init_stable_state();
    let err =
        super::rpc_eth_get_storage_at(vec![0u8; 19], vec![0u8; 32], super::RpcBlockTagView::Latest)
            .expect_err("bad address");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "address must be 20 bytes");
    let err =
        super::rpc_eth_get_storage_at(vec![0u8; 20], vec![0u8; 31], super::RpcBlockTagView::Latest)
            .expect_err("bad slot");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "slot must be 32 bytes");
}

#[test]
fn expected_nonce_by_address_rejects_bytes32_encoded_principal() {
    init_stable_state();
    let err = super::expected_nonce_by_address(vec![0u8; 32]).expect_err("bad address");
    assert_eq!(
        err,
        "address must be 20 bytes (got 32; this looks like bytes32-encoded principal)"
    );
}

#[test]
fn rpc_eth_call_object_success_and_estimate_gas_work() {
    init_stable_state();
    let from = [0x77u8; 20];
    evm_db::stable_state::with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(from),
            AccountVal::from_parts(0, [0xffu8; 32], [0u8; 32]),
        );
    });
    let call = super::RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: Some(from.to_vec()),
        gas: Some(30_000),
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: Some(vec![0u8; 32]),
        data: Some(Vec::new()),
    };
    let out = super::rpc_eth_call_object(call.clone()).expect("call object");
    assert_eq!(out.status, 1);
    assert!(out.gas_used > 0);
    assert!(out.revert_data.is_none());
    let gas = super::rpc_eth_estimate_gas_object(call).expect("estimate");
    assert!(gas > 0);
}

#[test]
fn rpc_eth_call_object_rejects_bad_address_len() {
    init_stable_state();
    let err = super::rpc_eth_call_object(super::RpcCallObjectView {
        to: Some(vec![0u8; 19]),
        from: None,
        gas: None,
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: None,
        data: None,
    })
    .expect_err("bad to");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "to must be 20 bytes");
    let err = super::rpc_eth_call_object(super::RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: Some(vec![0u8; 19]),
        gas: None,
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: None,
        data: None,
    })
    .expect_err("bad from");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "from must be 20 bytes");
}

#[test]
fn rpc_eth_call_object_rejects_bad_value_len() {
    init_stable_state();
    let err = super::rpc_eth_call_object(super::RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: None,
        gas: None,
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: Some(vec![0u8; 31]),
        data: None,
    })
    .expect_err("bad value");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "value must be 32 bytes");
}

fn is_machine_code(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch == '.' || ch == '_' || ch.is_ascii_lowercase() || ch.is_ascii_digit())
}

fn inspect_allowlist_methods() -> BTreeSet<&'static str> {
    let mut out = BTreeSet::new();
    for policy in INSPECT_METHOD_POLICIES {
        out.insert(policy.method);
    }
    out
}

fn did_update_methods() -> BTreeSet<String> {
    let did = include_str!("../evm_canister.did");
    let mut out = BTreeSet::new();
    let mut in_service = false;
    let mut stmt = String::new();
    for line in did.lines() {
        let trimmed = line.trim();
        if !in_service {
            if trimmed.starts_with("service ") || trimmed.starts_with("service:") {
                in_service = true;
            }
            continue;
        }
        if trimmed == "}" {
            break;
        }
        if trimmed.is_empty() {
            continue;
        }
        if !stmt.is_empty() {
            stmt.push(' ');
        }
        stmt.push_str(trimmed);
        if !trimmed.ends_with(';') {
            continue;
        }
        if stmt.contains(" : (") && stmt.contains("-> (") && !stmt.contains(" query") {
            if let Some((name, _)) = stmt.split_once(" : (") {
                out.insert(name.trim().to_string());
            }
        }
        stmt.clear();
    }
    out
}

#[test]
fn with_state_mut_blocks_avoid_async_and_timer_side_effects() {
    let source = include_str!("lib.rs");
    for (start, _) in source.match_indices("with_state_mut(|") {
        let tail = &source[start..];
        let Some(rel_end) = tail.find("});") else {
            continue;
        };
        let end = start + rel_end + 3;
        let segment = &source[start..end];
        assert!(
            !segment.contains("ic_cdk_timers::set_timer("),
            "set_timer must be outside with_state_mut block"
        );
        assert!(
            !segment.contains("ic_cdk_timers::set_timer_interval("),
            "set_timer_interval must be outside with_state_mut block"
        );
        assert!(
            !segment.contains(".await"),
            "await must not appear inside with_state_mut block"
        );
    }
}

#[test]
fn request_dispatch_status_to_view_maps_dispatch_states() {
    assert_eq!(
        super::request_dispatch_status_to_view(UnwrapRequestStatus::Queued),
        super::RequestDispatchStatusView::Queued
    );
    assert_eq!(
        super::request_dispatch_status_to_view(UnwrapRequestStatus::Dispatching),
        super::RequestDispatchStatusView::Dispatching
    );
    assert_eq!(
        super::request_dispatch_status_to_view(UnwrapRequestStatus::Dispatched),
        super::RequestDispatchStatusView::Dispatched
    );
    assert_eq!(
        super::request_dispatch_status_to_view(UnwrapRequestStatus::DispatchFailed),
        super::RequestDispatchStatusView::DispatchFailed
    );
    assert_eq!(
        super::icp_update_request_status_to_view(IcpUpdateRequestStatus::DispatchUncertain),
        super::RequestDispatchStatusView::DispatchUncertain
    );
}

#[test]
fn get_unwrap_dispatch_overview_returns_status_without_vault_id() {
    init_stable_state();
    let request_id = TxId([0x11u8; 32]);
    with_state_mut(|state| {
        let mut meta = *state.unwrap_dispatch_meta.get();
        let seq = meta.push();
        state.unwrap_dispatch_meta.set(meta);
        state.unwrap_dispatch_queue.insert(seq, request_id);
        state.unwrap_requests.insert(
            request_id,
            sample_unwrap_request(UnwrapRequestStatus::Dispatched, None, 1),
        );
    });

    let result = super::get_unwrap_dispatch_overview(request_id.0.to_vec()).expect("result");
    assert_eq!(result.status, super::RequestDispatchStatusView::Dispatched);
    let stored = with_state(|state| state.unwrap_requests.get(&request_id));
    assert!(stored.is_some());
}

#[test]
fn get_unwrap_dispatch_overview_returns_dispatch_failed_for_decode_marker() {
    init_stable_state();
    let request_id = TxId([0x12u8; 32]);
    with_state_mut(|state| {
        state.unwrap_requests.insert(
            request_id,
            sample_unwrap_request(
                UnwrapRequestStatus::DispatchFailed,
                Some(super::UNWRAP_DECODE_FAILURE_CODE),
                0,
            ),
        );
    });

    let result = super::get_unwrap_dispatch_overview(request_id.0.to_vec()).expect("result");
    assert_eq!(
        result.status,
        super::RequestDispatchStatusView::DispatchFailed
    );
}

#[test]
fn get_unwrap_dispatch_overview_normalizes_decode_failure_error_code() {
    init_stable_state();
    let request_id = TxId([0x13u8; 32]);
    with_state_mut(|state| {
        state.unwrap_requests.insert(
            request_id,
            sample_unwrap_request(
                UnwrapRequestStatus::DispatchFailed,
                Some(super::UNWRAP_DECODE_FAILURE_CODE),
                0,
            ),
        );
    });

    let result = super::get_unwrap_dispatch_overview(request_id.0.to_vec()).expect("result");
    assert_eq!(
        result.error,
        Some(super::UNWRAP_QUARANTINE_ERROR.to_string())
    );
    let stored = with_state(|state| state.unwrap_requests.get(&request_id));
    assert_eq!(
        stored.and_then(|value| value.error_code),
        Some(super::UNWRAP_DECODE_FAILURE_CODE.to_string())
    );
}

#[test]
fn unwrap_request_ids_for_tx_derives_one_id_per_unwrap_log() {
    init_stable_state();
    let tx_id = TxId([0x21u8; 32]);
    let amount = [0x44u8; 32];
    let unwrap_topic = hash::keccak256(b"KasaneUnwrapRequest(bytes)");
    let logs = vec![
        log_entry_from_parts(
            WRAP_PRECOMPILE_ADDRESS.into_array(),
            vec![unwrap_topic],
            unwrap_log_data(&[0x01, 0x02], amount, &[0x03, 0x04]),
        ),
        log_entry_from_parts([0x77u8; 20], vec![[0x88u8; 32]], vec![0x99]),
        log_entry_from_parts(
            WRAP_PRECOMPILE_ADDRESS.into_array(),
            vec![unwrap_topic],
            unwrap_log_data(&[0x05, 0x06], amount, &[0x07, 0x08]),
        ),
        log_entry_from_parts(
            NATIVE_WITHDRAW_PRECOMPILE_ADDRESS.into_array(),
            vec![hash::keccak256(b"KasaneNativeWithdrawalRequest(bytes)")],
            native_withdraw_log_data(amount, &[0x09, 0x0a]),
        ),
    ];
    with_state_mut(|state| {
        let receipt = ReceiptLike {
            tx_id,
            block_number: 7,
            tx_index: 0,
            status: 1,
            gas_used: 1,
            effective_gas_price: 1,
            l1_data_fee: 0,
            operator_fee: 0,
            total_fee: 0,
            return_data_hash: [0u8; 32],
            return_data: Vec::new(),
            contract_address: None,
            logs,
        };
        let ptr = state
            .blob_store
            .store_bytes(receipt.to_bytes().as_ref())
            .expect("store receipt");
        state.receipts.insert(tx_id, ptr);
    });

    let request_ids = super::unwrap_request_ids_for_tx(&tx_id)
        .into_iter()
        .map(|value| value.0.to_vec())
        .collect::<Vec<_>>();
    assert_eq!(request_ids.len(), 3);
    assert_ne!(request_ids[0], request_ids[1]);
    assert_ne!(request_ids[1], request_ids[2]);
    assert_eq!(
        request_ids[0],
        super::derive_log_request_id(&tx_id, 0)
            .expect("first id")
            .0
            .to_vec()
    );
    assert_eq!(
        request_ids[1],
        super::derive_log_request_id(&tx_id, 2)
            .expect("second id")
            .0
            .to_vec()
    );
    assert_eq!(
        request_ids[2],
        super::derive_log_request_id(&tx_id, 3)
            .expect("native id")
            .0
            .to_vec()
    );
}

#[test]
fn record_unwrap_requests_from_block_stores_native_withdraw_logs_as_gross() {
    init_stable_state();
    let tx_id = TxId([0x24u8; 32]);
    let amount = [0x42u8; 32];
    with_state_mut(|state| {
        let receipt = ReceiptLike {
            tx_id,
            block_number: 10,
            tx_index: 0,
            status: 1,
            gas_used: 1,
            effective_gas_price: 1,
            l1_data_fee: 0,
            operator_fee: 0,
            total_fee: 0,
            return_data_hash: [0u8; 32],
            return_data: Vec::new(),
            contract_address: None,
            logs: vec![log_entry_from_parts(
                NATIVE_WITHDRAW_PRECOMPILE_ADDRESS.into_array(),
                vec![hash::keccak256(b"KasaneNativeWithdrawalRequest(bytes)")],
                native_withdraw_log_data(amount, &[0x09, 0x0a]),
            )],
        };
        let ptr = state
            .blob_store
            .store_bytes(receipt.to_bytes().as_ref())
            .expect("store receipt");
        state.receipts.insert(tx_id, ptr);
    });

    super::record_unwrap_requests_from_block(&[tx_id]);

    let request_id = super::derive_log_request_id(&tx_id, 0).expect("request id");
    with_state(|state| {
        let req = state.unwrap_requests.get(&request_id).expect("request");
        assert_eq!(req.asset_id, super::NATIVE_WITHDRAW_ASSET_MARKER);
        assert_eq!(req.amount, amount);
        assert_eq!(state.unwrap_dispatch_queue.len(), 1);
    });
}

#[test]
fn record_icp_update_requests_from_block_stores_update_intent_logs() {
    init_stable_state();
    let tx_id = TxId([0x25u8; 32]);
    let target = Principal::self_authenticating(b"update-target");
    let caller_principal = Principal::self_authenticating(b"update-caller");
    let caller_evm = [0x33u8; 20];
    let method = "write_state";
    let arg = vec![0x44, 0x49, 0x44, 0x4c];
    with_state_mut(|state| {
        let raw = encode_ic_synthetic_input(&IcSyntheticTxInput {
            to: Some([0x44u8; 20]),
            value: [0u8; 32],
            gas_limit: 100_000,
            nonce: 0,
            max_fee_per_gas: 1,
            max_priority_fee_per_gas: 1,
            data: Vec::new(),
        });
        state.tx_store.insert(
            tx_id,
            StoredTxBytes::new_with_fees(
                tx_id,
                TxKind::IcSynthetic,
                raw,
                Some(caller_evm),
                vec![0xa1],
                caller_principal.as_slice().to_vec(),
                1,
                1,
                true,
            ),
        );
        let receipt = ReceiptLike {
            tx_id,
            block_number: 10,
            tx_index: 0,
            status: 1,
            gas_used: 1,
            effective_gas_price: 1,
            l1_data_fee: 0,
            operator_fee: 0,
            total_fee: 0,
            return_data_hash: [0u8; 32],
            return_data: Vec::new(),
            contract_address: None,
            logs: vec![log_entry_from_parts(
                ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS.into_array(),
                vec![hash::keccak256(b"KasaneIcpUpdateIntent(bytes)")],
                icp_update_log_data(target.as_slice(), method, &arg),
            )],
        };
        let ptr = state
            .blob_store
            .store_bytes(receipt.to_bytes().as_ref())
            .expect("store receipt");
        state.receipts.insert(tx_id, ptr);
    });

    super::record_icp_update_requests_from_block(&[tx_id]);

    let request_id = super::derive_log_request_id(&tx_id, 0).expect("request id");
    with_state(|state| {
        let req = state.icp_update_requests.get(&request_id).expect("request");
        assert_eq!(req.target, target.as_slice());
        assert_eq!(req.method, method);
        assert_eq!(req.arg, arg);
        assert_eq!(req.tx_id, tx_id);
        assert_eq!(req.block_number, 10);
        assert_eq!(req.tx_index, 0);
        assert_eq!(req.log_index, 0);
        assert_eq!(req.tx_kind, TxKind::IcSynthetic);
        assert_eq!(req.evm_sender, caller_evm);
        assert_eq!(req.ic_caller, Some(caller_principal.as_slice().to_vec()));
        assert_eq!(req.status, IcpUpdateRequestStatus::Queued);
        assert_eq!(state.icp_update_dispatch_queue.len(), 1);
    });
}

#[test]
fn record_icp_update_requests_from_block_recovers_eth_signed_sender() {
    init_stable_state();
    let tx_id = TxId([0x35u8; 32]);
    let target = Principal::self_authenticating(b"update-target");
    let method = "write_state";
    let arg = vec![0x44, 0x49, 0x44, 0x4c];
    let raw = vec![
        248, 102, 128, 132, 119, 53, 148, 0, 130, 82, 8, 148, 17, 17, 17, 17, 17, 17, 17, 17, 17,
        17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 128, 128, 131, 146, 134, 195, 160, 241, 201,
        193, 221, 229, 28, 92, 162, 210, 218, 13, 176, 33, 64, 247, 25, 214, 133, 94, 246, 120,
        240, 140, 18, 182, 27, 238, 1, 52, 139, 197, 154, 160, 62, 82, 38, 75, 27, 220, 41, 102,
        113, 163, 180, 104, 253, 253, 192, 16, 51, 63, 241, 248, 30, 9, 113, 78, 228, 134, 160,
        232, 226, 115, 9, 23,
    ];
    let expected_sender = [
        0x68, 0x1b, 0x48, 0x23, 0x1b, 0x2e, 0xf4, 0x4c, 0x22, 0xf6, 0xd7, 0x80, 0x4a, 0xff, 0x41,
        0x06, 0x66, 0xd2, 0x14, 0xe3,
    ];
    with_state_mut(|state| {
        state.tx_store.insert(
            tx_id,
            StoredTxBytes::new_with_fees(
                tx_id,
                TxKind::EthSigned,
                raw,
                None,
                vec![0xa1],
                Principal::self_authenticating(b"ignored-eth-caller")
                    .as_slice()
                    .to_vec(),
                2_000_000_000,
                0,
                false,
            ),
        );
        let receipt = ReceiptLike {
            tx_id,
            block_number: 11,
            tx_index: 1,
            status: 1,
            gas_used: 1,
            effective_gas_price: 1,
            l1_data_fee: 0,
            operator_fee: 0,
            total_fee: 0,
            return_data_hash: [0u8; 32],
            return_data: Vec::new(),
            contract_address: None,
            logs: vec![log_entry_from_parts(
                ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS.into_array(),
                vec![hash::keccak256(b"KasaneIcpUpdateIntent(bytes)")],
                icp_update_log_data(target.as_slice(), method, &arg),
            )],
        };
        let ptr = state
            .blob_store
            .store_bytes(receipt.to_bytes().as_ref())
            .expect("store receipt");
        state.receipts.insert(tx_id, ptr);
    });

    super::record_icp_update_requests_from_block(&[tx_id]);

    let request_id = super::derive_log_request_id(&tx_id, 0).expect("request id");
    with_state(|state| {
        let req = state.icp_update_requests.get(&request_id).expect("request");
        assert_eq!(req.tx_kind, TxKind::EthSigned);
        assert_eq!(req.evm_sender, expected_sender);
        assert_eq!(req.ic_caller, None);
        assert_eq!(req.status, IcpUpdateRequestStatus::Queued);
    });
}

#[test]
fn pop_next_icp_update_request_marks_dispatching() {
    init_stable_state();
    let request_id = TxId([0x26u8; 32]);
    with_state_mut(|state| {
        state.icp_update_requests.insert(
            request_id,
            test_icp_update_request(
                request_id,
                vec![1, 2, 3],
                "write_state",
                IcpUpdateRequestStatus::Queued,
            ),
        );
        let mut meta = *state.icp_update_dispatch_meta.get();
        let seq = meta.push();
        state.icp_update_dispatch_meta.set(meta);
        state.icp_update_dispatch_queue.insert(seq, request_id);
    });

    let popped = pop_next_icp_update_request(99)
        .expect("pop ok")
        .expect("request");
    assert_eq!(popped.0, request_id);
    assert_eq!(popped.1.status, IcpUpdateRequestStatus::Dispatching);
    assert_eq!(popped.1.call_started_at_time, 99);
    with_state(|state| {
        let stored = state.icp_update_requests.get(&request_id).expect("stored");
        assert_eq!(stored.status, IcpUpdateRequestStatus::Dispatching);
        assert_eq!(stored.call_started_at_time, 99);
        assert!(state.icp_update_dispatch_queue.is_empty());
    });
}

#[test]
fn icp_update_dispatch_scheduler_does_not_arm_while_in_flight() {
    super::reset_icp_update_dispatch_scheduler_for_tests(true);

    super::schedule_icp_update_dispatch();

    assert_eq!(
        super::icp_update_dispatch_scheduler_state_for_tests(),
        (true, 0)
    );
}

#[test]
fn icp_update_dispatch_completion_keeps_single_flight_when_queue_remains() {
    init_stable_state();
    super::reset_icp_update_dispatch_scheduler_for_tests(true);
    with_state_mut(|state| {
        let mut meta = *state.icp_update_dispatch_meta.get();
        let seq = meta.push();
        state.icp_update_dispatch_meta.set(meta);
        state
            .icp_update_dispatch_queue
            .insert(seq, TxId([0x29u8; 32]));
    });

    super::complete_icp_update_dispatch_tick();

    assert_eq!(
        super::icp_update_dispatch_scheduler_state_for_tests(),
        (true, 1)
    );
}

#[test]
fn icp_update_dispatch_finish_reschedules_race_queued_work() {
    init_stable_state();
    super::reset_icp_update_dispatch_scheduler_for_tests(true);
    with_state_mut(|state| {
        let mut meta = *state.icp_update_dispatch_meta.get();
        let seq = meta.push();
        state.icp_update_dispatch_meta.set(meta);
        state
            .icp_update_dispatch_queue
            .insert(seq, TxId([0x2au8; 32]));
    });

    super::finish_icp_update_dispatch_tick();

    assert_eq!(
        super::icp_update_dispatch_scheduler_state_for_tests(),
        (true, 1)
    );
}

#[test]
fn icp_update_dispatch_rejects_allowlist_miss_without_calling_target() {
    init_stable_state();
    let mut req = test_icp_update_request(
        TxId([0x2cu8; 32]),
        Principal::self_authenticating(b"update-target")
            .as_slice()
            .to_vec(),
        "write_state",
        IcpUpdateRequestStatus::Dispatching,
    );
    req.call_started_at_time = 1;

    let out = run_ready_future(super::dispatch_icp_update_request_internal(req));

    assert_eq!(out.status, IcpUpdateRequestStatus::DispatchFailed);
    assert_eq!(out.reply, None);
    assert_eq!(out.error_code, Some("ic_update.allowlist_miss".to_string()));
}

#[test]
fn icp_update_success_outcome_omits_large_reply_without_failure() {
    let out = super::icp_update_success_outcome(vec![0u8; MAX_RETURN_DATA + 1]);

    assert_eq!(out.status, IcpUpdateRequestStatus::Dispatched);
    assert_eq!(out.reply, None);
    assert_eq!(
        out.error_code,
        Some(super::ICP_UPDATE_REPLY_OMITTED_TOO_LARGE.to_string())
    );
}

#[test]
fn icp_update_envelope_contains_sender_trace() {
    let request_id = TxId([0x60u8; 32]);
    let target = Principal::self_authenticating(b"update-target");
    let req = test_icp_update_request(
        request_id,
        target.as_slice().to_vec(),
        "write_state",
        IcpUpdateRequestStatus::Dispatching,
    );

    let envelope = super::icp_update_envelope(&req);

    assert_eq!(envelope.version, 1);
    assert_eq!(envelope.chain_id, evm_db::chain_data::constants::CHAIN_ID);
    assert_eq!(envelope.request_id, req.request_id.0.to_vec());
    assert_eq!(envelope.tx_id, req.tx_id.0.to_vec());
    assert_eq!(envelope.block_number, req.block_number);
    assert_eq!(envelope.tx_index, req.tx_index);
    assert_eq!(envelope.log_index, req.log_index);
    assert_eq!(envelope.tx_kind, super::IcpUpdateTxKindView::IcSynthetic);
    assert_eq!(envelope.evm_sender, req.evm_sender.to_vec());
    assert_eq!(envelope.ic_caller, Some(Principal::from_slice(&[0x53])));
    assert_eq!(envelope.arg, req.arg);
}

#[test]
fn icp_update_uncertain_error_classifier_marks_bounded_wait_timeout_unknown_only() {
    assert!(super::is_icp_update_uncertain_call_error(
        &CallFailed::CallRejected(CallRejected::with_rejection(
            RejectCode::SysUnknown as u32,
            "timeout".to_string(),
        ))
    ));
    assert!(!super::is_icp_update_uncertain_call_error(
        &CallFailed::CallRejected(CallRejected::with_rejection(
            RejectCode::SysTransient as u32,
            "temporary routing failure".to_string(),
        ))
    ));
    assert!(!super::is_icp_update_uncertain_call_error(
        &CallFailed::CallRejected(CallRejected::with_rejection(
            RejectCode::CanisterReject as u32,
            "bad input".to_string(),
        ))
    ));
    assert!(!super::is_icp_update_uncertain_call_error(
        &CallFailed::CallPerformFailed(CallPerformFailed)
    ));
}

#[test]
fn get_icp_update_request_returns_dispatch_result() {
    init_stable_state();
    let request_id = TxId([0x27u8; 32]);
    let target = Principal::self_authenticating(b"update-target");
    with_state_mut(|state| {
        state.icp_update_requests.insert(request_id, {
            let mut req = test_icp_update_request(
                request_id,
                target.as_slice().to_vec(),
                "write_state",
                IcpUpdateRequestStatus::Dispatched,
            );
            req.reply = Some(vec![0x01, 0x02]);
            req.updated_at = 99;
            req.call_started_at_time = 88;
            req
        });
    });

    let view = super::get_icp_update_request(request_id.0.to_vec()).expect("view");

    assert_eq!(view.request_id, request_id.0.to_vec());
    assert_eq!(view.target, target);
    assert_eq!(view.method, "write_state");
    assert_eq!(view.status, super::RequestDispatchStatusView::Dispatched);
    assert_eq!(view.reply, Some(vec![0x01, 0x02]));
    assert_eq!(view.error, None);
    assert_eq!(view.updated_at, 99);
}

#[test]
fn trim_icp_update_requests_removes_oldest_completed_only() {
    init_stable_state();
    with_state_mut(|state| {
        let queued_id = TxId([0x61u8; 32]);
        state.icp_update_requests.insert(
            queued_id,
            test_icp_update_request(
                queued_id,
                vec![1],
                "write_state",
                IcpUpdateRequestStatus::Queued,
            ),
        );
        for idx in 0..=super::MAX_ICP_UPDATE_REQUESTS {
            let mut raw = [0x62u8; 32];
            raw[24..32].copy_from_slice(&(idx as u64).to_be_bytes());
            let request_id = TxId(raw);
            let mut req = test_icp_update_request(
                request_id,
                vec![2],
                "write_state",
                IcpUpdateRequestStatus::Dispatched,
            );
            req.updated_at = idx as u64;
            state.icp_update_requests.insert(request_id, req);
        }

        super::trim_icp_update_requests(state);

        assert_eq!(
            state.icp_update_requests.len(),
            super::MAX_ICP_UPDATE_REQUESTS as u64
        );
        assert!(state.icp_update_requests.get(&queued_id).is_some());
        let mut oldest = [0x62u8; 32];
        oldest[24..32].copy_from_slice(&0u64.to_be_bytes());
        assert!(state.icp_update_requests.get(&TxId(oldest)).is_none());
    });
}

#[test]
fn get_icp_update_request_returns_omitted_large_reply_reason() {
    init_stable_state();
    let request_id = TxId([0x2bu8; 32]);
    let target = Principal::self_authenticating(b"update-target");
    with_state_mut(|state| {
        state.icp_update_requests.insert(request_id, {
            let mut req = test_icp_update_request(
                request_id,
                target.as_slice().to_vec(),
                "write_state",
                IcpUpdateRequestStatus::Dispatched,
            );
            req.error_code = Some(super::ICP_UPDATE_REPLY_OMITTED_TOO_LARGE.to_string());
            req.updated_at = 99;
            req.call_started_at_time = 88;
            req
        });
    });

    let view = super::get_icp_update_request(request_id.0.to_vec()).expect("view");

    assert_eq!(view.status, super::RequestDispatchStatusView::Dispatched);
    assert_eq!(view.reply, None);
    assert_eq!(
        view.error,
        Some(super::ICP_UPDATE_REPLY_OMITTED_TOO_LARGE.to_string())
    );
}

#[test]
fn recover_icp_update_dispatch_marks_dispatching_uncertain_without_requeue() {
    init_stable_state();
    let request_id = TxId([0x28u8; 32]);
    with_state_mut(|state| {
        state.icp_update_requests.insert(request_id, {
            let mut req = test_icp_update_request(
                request_id,
                Principal::self_authenticating(b"update-target")
                    .as_slice()
                    .to_vec(),
                "write_state",
                IcpUpdateRequestStatus::Dispatching,
            );
            req.call_started_at_time = 1;
            req
        });
    });

    let needs_schedule = super::recover_icp_update_dispatch_state_after_upgrade(99);

    assert!(!needs_schedule);
    with_state(|state| {
        let stored = state.icp_update_requests.get(&request_id).expect("stored");
        assert_eq!(stored.status, IcpUpdateRequestStatus::DispatchUncertain);
        assert_eq!(stored.updated_at, 99);
        assert_eq!(
            stored.error_code,
            Some("ic_update.dispatch_uncertain".to_string())
        );
        assert!(state.icp_update_dispatch_queue.is_empty());
    });
}

#[test]
fn get_unwrap_request_ids_by_tx_id_returns_ids_for_matching_logs() {
    init_stable_state();
    let tx_id = TxId([0x22u8; 32]);
    let amount = [0x55u8; 32];
    let unwrap_topic = hash::keccak256(b"KasaneUnwrapRequest(bytes)");
    let other_log = log_entry_from_parts([0x11u8; 20], vec![[0x99u8; 32]], vec![0x01, 0x02]);
    with_state_mut(|state| {
        let receipt = ReceiptLike {
            tx_id,
            block_number: 8,
            tx_index: 0,
            status: 1,
            gas_used: 1,
            effective_gas_price: 1,
            l1_data_fee: 0,
            operator_fee: 0,
            total_fee: 0,
            return_data_hash: [0u8; 32],
            return_data: Vec::new(),
            contract_address: None,
            logs: vec![
                other_log.clone(),
                log_entry_from_parts(
                    WRAP_PRECOMPILE_ADDRESS.into_array(),
                    vec![unwrap_topic],
                    unwrap_log_data(&[0x44, 0x55, 0x66], amount, &[0x77, 0x88]),
                ),
                other_log,
            ],
        };
        let ptr = state
            .blob_store
            .store_bytes(receipt.to_bytes().as_ref())
            .expect("store receipt");
        state.receipts.insert(tx_id, ptr);
    });

    let ids = super::get_unwrap_request_ids_by_tx_id(tx_id.0.to_vec());
    assert_eq!(ids.len(), 1);
    assert_eq!(
        ids[0],
        super::derive_log_request_id(&tx_id, 1)
            .expect("request id")
            .0
            .to_vec()
    );
}

#[test]
fn get_unwrap_request_ids_by_eth_tx_hash_resolves_indexed_internal_tx() {
    init_stable_state();
    let raw = vec![0x02, 0x01, 0x02, 0x03];
    let tx_id = {
        let mut input = Vec::new();
        input.extend_from_slice(b"ic-evm:storedtx:v2");
        input.push(TxKind::EthSigned.to_u8());
        input.extend_from_slice(&raw);
        TxId(hash::keccak256(&input))
    };
    let eth_tx_hash = hash::keccak256(&raw);
    let amount = [0x66u8; 32];
    let unwrap_topic = hash::keccak256(b"KasaneUnwrapRequest(bytes)");
    with_state_mut(|state| {
        let stored = StoredTxBytes::new_with_fees(
            tx_id,
            TxKind::EthSigned,
            raw,
            None,
            Vec::new(),
            Vec::new(),
            1,
            1,
            true,
        );
        state.tx_store.insert(tx_id, stored);
        state.eth_tx_hash_index.insert(TxId(eth_tx_hash), tx_id);
        let receipt = ReceiptLike {
            tx_id,
            block_number: 9,
            tx_index: 0,
            status: 1,
            gas_used: 1,
            effective_gas_price: 1,
            l1_data_fee: 0,
            operator_fee: 0,
            total_fee: 0,
            return_data_hash: [0u8; 32],
            return_data: Vec::new(),
            contract_address: None,
            logs: vec![log_entry_from_parts(
                WRAP_PRECOMPILE_ADDRESS.into_array(),
                vec![unwrap_topic],
                unwrap_log_data(&[0xaa, 0xbb], amount, &[0xcc]),
            )],
        };
        let ptr = state
            .blob_store
            .store_bytes(receipt.to_bytes().as_ref())
            .expect("store receipt");
        state.receipts.insert(tx_id, ptr);
    });

    let ids = super::get_unwrap_request_ids_by_eth_tx_hash(eth_tx_hash.to_vec());
    assert_eq!(ids.len(), 1);
    assert_eq!(
        ids[0],
        super::derive_log_request_id(&tx_id, 0)
            .expect("request id")
            .0
            .to_vec()
    );
    assert!(super::get_unwrap_request_ids_by_eth_tx_hash(vec![0u8; 31]).is_empty());
    assert!(super::get_unwrap_request_ids_by_eth_tx_hash(vec![0u8; 32]).is_empty());
}

#[test]
fn raw_stable_corruption_does_not_trap_query_or_dispatch_for_unwrap_request() {
    init_stable_state();
    let request_id = TxId([0x14u8; 32]);
    let request = UnwrapDispatchRequest {
        asset_id: vec![0xB2u8; 11],
        amount: [0xC3u8; 32],
        recipient: vec![0xD4u8; 19],
        status: UnwrapRequestStatus::Queued,
        ledger_tx_id: Some(vec![0xE5u8; 17]),
        error_code: Some("wrap.integration.gateway.raw-corrupt.7f3e2c1b".to_string()),
        updated_at: 987_654_333,
        transfer_created_at_time: 987_654_334,
    };
    let encoded = request.to_bytes().into_owned();
    with_state_mut(|state| {
        let mut meta = *state.unwrap_dispatch_meta.get();
        let seq = meta.push();
        state.unwrap_dispatch_meta.set(meta);
        state.unwrap_dispatch_queue.insert(seq, request_id);
        state.unwrap_requests.insert(request_id, request);
    });

    let memory = get_memory(AppMemoryId::UnwrapRequests);
    let pages = memory.size();
    assert!(pages > 0, "unwrap request memory pages must be allocated");
    let mut dump = vec![0u8; (pages * WASM_PAGE_SIZE_BYTES) as usize];
    memory.read(0, &mut dump);
    let candidates = find_subsequence_positions(&dump, &encoded);
    assert!(
        !candidates.is_empty(),
        "encoded unwrap request bytes must be present in stable memory"
    );

    let mut corrupted_target = false;
    for encoded_offset in candidates.into_iter() {
        let checksum_last = encoded_offset + encoded.len() - 1;
        let original = dump[checksum_last];
        memory.write(checksum_last as u64, &[original ^ 0x01]);
        let is_decode_failed = with_state(|state| {
            state.unwrap_requests.get(&request_id).is_some_and(|value| {
                value.status == UnwrapRequestStatus::DispatchFailed
                    && value.error_code.as_deref() == Some(super::UNWRAP_DECODE_FAILURE_CODE)
            })
        });
        if is_decode_failed {
            corrupted_target = true;
            break;
        }
        memory.write(checksum_last as u64, &[original]);
    }
    assert!(
        corrupted_target,
        "failed to corrupt the inserted unwrap request payload"
    );

    let query_out = catch_unwind(AssertUnwindSafe(|| {
        super::get_unwrap_dispatch_overview(request_id.0.to_vec())
    }));
    assert!(
        query_out.is_ok(),
        "query path must not trap on raw corruption"
    );
    let query_result = query_out
        .expect("query must not panic")
        .expect("result must exist");
    assert_eq!(
        query_result.status,
        super::RequestDispatchStatusView::DispatchFailed
    );
    assert_eq!(
        query_result.error,
        Some(super::UNWRAP_QUARANTINE_ERROR.to_string())
    );

    let pop_out = catch_unwind(AssertUnwindSafe(|| pop_next_dispatch_request(123)));
    assert!(
        pop_out.is_ok(),
        "dispatch path must not trap on raw corruption"
    );
    let pop_err = pop_out
        .expect("pop call must not panic")
        .expect_err("corrupted unwrap request must be quarantined");
    assert!(pop_err.starts_with("wrap.dispatch.quarantined:"));

    let (stored, queue_len) = with_state(|state| {
        (
            state.unwrap_requests.get(&request_id),
            state.unwrap_dispatch_queue.len(),
        )
    });
    assert_eq!(queue_len, 0);
    assert_eq!(
        stored.and_then(|value| value.error_code),
        Some(super::UNWRAP_QUARANTINE_ERROR.to_string())
    );
}

#[test]
fn raw_stable_corruption_does_not_trap_query_or_recovery_for_wrap_request() {
    init_stable_state();
    let request_id = TxId([0x15u8; 32]);
    let request = sample_wrap_request(RequestStatus::Queued);
    let encoded = request.to_bytes().into_owned();
    with_state_mut(|state| {
        state.wrap_requests.insert(request_id, request);
    });

    let memory = get_memory(AppMemoryId::WrapRequests);
    let pages = memory.size();
    assert!(pages > 0, "wrap request memory pages must be allocated");
    let mut dump = vec![0u8; (pages * WASM_PAGE_SIZE_BYTES) as usize];
    memory.read(0, &mut dump);
    let candidates = find_subsequence_positions(&dump, &encoded);
    assert!(
        !candidates.is_empty(),
        "encoded wrap request bytes must be present in stable memory"
    );

    let mut corrupted_target = false;
    for encoded_offset in candidates.into_iter() {
        let checksum_last = encoded_offset + encoded.len() - 1;
        let original = dump[checksum_last];
        memory.write(checksum_last as u64, &[original ^ 0x01]);
        let is_decode_failed = with_state(|state| {
            state.wrap_requests.get(&request_id).is_some_and(|value| {
                value.result.status == RequestStatus::Failed
                    && value.result.error_code.as_deref() == Some(WRAP_DECODE_FAILURE_CODE)
            })
        });
        if is_decode_failed {
            corrupted_target = true;
            break;
        }
        memory.write(checksum_last as u64, &[original]);
    }
    assert!(
        corrupted_target,
        "failed to corrupt the inserted wrap request payload"
    );

    let query_out = catch_unwind(AssertUnwindSafe(|| {
        super::get_request(request_id.0.to_vec())
    }));
    assert!(
        query_out.is_ok(),
        "query path must not trap on raw corruption"
    );
    let overview = query_out
        .expect("query must not panic")
        .expect("overview must exist");
    assert_eq!(overview.status, RequestStatus::Failed);
    assert_eq!(overview.stage, Some(super::RequestStageView::Failed));
    assert_eq!(
        overview.error.map(|error| error.code),
        Some(WRAP_DECODE_FAILURE_CODE.to_string())
    );

    let recover_out = catch_unwind(AssertUnwindSafe(
        super::recover_wrap_worker_state_after_upgrade,
    ));
    assert!(
        recover_out.is_ok(),
        "worker recovery must not trap on raw corruption"
    );
    let repair_out = catch_unwind(AssertUnwindSafe(|| {
        super::repair_stale_operations(987_654_321)
    }));
    assert!(
        repair_out.is_ok(),
        "stale repair must not trap on raw corruption"
    );
}

#[test]
fn raw_stable_corruption_does_not_trap_reads_for_chain_state() {
    const STABLE_CELL_LEN_OFFSET: u64 = 4;
    const STABLE_CELL_VALUE_OFFSET: u64 = 8;

    init_stable_state();
    set_migration_not_pending_for_test();

    let encoded = with_state(|state| state.chain_state.get().to_bytes().into_owned());
    let memory = get_memory(AppMemoryId::ChainState);
    let original_len = u32::try_from(encoded.len()).expect("chain_state len");
    memory.write(STABLE_CELL_LEN_OFFSET, &72u32.to_le_bytes());

    init_stable_state();

    let health_out = catch_unwind(AssertUnwindSafe(super::health));
    assert!(health_out.is_ok(), "health must not trap on raw corruption");
    let health = health_out.expect("health");
    assert!(!health.auto_production_enabled);
    assert!(!health.is_producing);
    assert!(!health.mining_scheduled);

    let ops_out = catch_unwind(AssertUnwindSafe(super::get_ops_status));
    assert!(
        ops_out.is_ok(),
        "ops status must not trap on raw corruption"
    );
    let ops = ops_out.expect("ops");
    assert!(ops.needs_migration);

    let reason = reject_write_reason().expect("write should be blocked");
    assert_eq!(reason, "ops.write.needs_migration");

    memory.write(STABLE_CELL_LEN_OFFSET, &original_len.to_le_bytes());
    memory.write(STABLE_CELL_VALUE_OFFSET, &encoded);
    init_stable_state();
    set_migration_not_pending_for_test();
    let mut restored = with_state(|state| *state.chain_state.get());
    restored.auto_production_enabled = true;
    with_state_mut(|state| {
        state.chain_state.set(restored);
    });
}

#[test]
fn raw_stable_corruption_in_head_blocks_writes_before_block_production() {
    const STABLE_CELL_LEN_OFFSET: u64 = 4;
    const STABLE_CELL_VALUE_OFFSET: u64 = 8;

    init_stable_state();
    set_migration_not_pending_for_test();

    let encoded = with_state(|state| state.head.get().to_bytes().into_owned());
    let memory = get_memory(AppMemoryId::Head);
    let original_len = u32::try_from(encoded.len()).expect("head len");
    memory.write(STABLE_CELL_LEN_OFFSET, &1u32.to_le_bytes());

    init_stable_state();

    let health_out = catch_unwind(AssertUnwindSafe(super::health));
    assert!(health_out.is_ok(), "health must not trap on raw corruption");
    let health = health_out.expect("health");
    assert_eq!(health.tip_number, 0);
    assert_eq!(health.tip_hash, vec![0u8; 32]);

    let reason = reject_write_reason().expect("write should be blocked");
    assert_eq!(reason, "ops.write.needs_migration");

    memory.write(STABLE_CELL_LEN_OFFSET, &original_len.to_le_bytes());
    memory.write(STABLE_CELL_VALUE_OFFSET, &encoded);
    init_stable_state();
    set_migration_not_pending_for_test();
}

#[test]
fn pop_next_dispatch_request_marks_dispatching_and_dequeues() {
    init_stable_state();
    let request_id = TxId([0x33u8; 32]);
    with_state_mut(|state| {
        let mut meta = *state.unwrap_dispatch_meta.get();
        let seq = meta.push();
        state.unwrap_dispatch_meta.set(meta);
        state.unwrap_dispatch_queue.insert(seq, request_id);
        state.unwrap_requests.insert(
            request_id,
            sample_unwrap_request(UnwrapRequestStatus::Queued, None, 1),
        );
    });

    let popped = pop_next_dispatch_request(123).expect("pop result");
    assert!(popped.is_some());
    let (id, req) = popped.expect("item");
    assert_eq!(id, request_id);
    assert_eq!(req.status, UnwrapRequestStatus::Dispatching);
    let stored = with_state(|state| state.unwrap_requests.get(&request_id));
    assert_eq!(
        stored.map(|v| v.status),
        Some(UnwrapRequestStatus::Dispatching)
    );
}

#[test]
fn pop_next_dispatch_request_quarantines_decode_failed_entry() {
    init_stable_state();
    let request_id = TxId([0x34u8; 32]);
    with_state_mut(|state| {
        let mut meta = *state.unwrap_dispatch_meta.get();
        let seq = meta.push();
        state.unwrap_dispatch_meta.set(meta);
        state.unwrap_dispatch_queue.insert(seq, request_id);
        state.unwrap_requests.insert(
            request_id,
            sample_unwrap_request(
                UnwrapRequestStatus::DispatchFailed,
                Some(super::UNWRAP_DECODE_FAILURE_CODE),
                1,
            ),
        );
    });

    let err = pop_next_dispatch_request(123).expect_err("must quarantine decode-failed entry");
    assert!(err.starts_with("wrap.dispatch.quarantined:"));
    let (stored, queue_len) = with_state(|state| {
        (
            state.unwrap_requests.get(&request_id),
            state.unwrap_dispatch_queue.len(),
        )
    });
    assert_eq!(queue_len, 0);
    assert_eq!(
        stored.as_ref().map(|v| v.status),
        Some(UnwrapRequestStatus::DispatchFailed)
    );
    assert_eq!(
        stored.and_then(|v| v.error_code),
        Some(super::UNWRAP_QUARANTINE_ERROR.to_string())
    );
}

#[test]
fn pop_next_dispatch_request_skips_already_quarantined_entry() {
    init_stable_state();
    let request_id = TxId([0x37u8; 32]);
    with_state_mut(|state| {
        let mut meta = *state.unwrap_dispatch_meta.get();
        let seq = meta.push();
        state.unwrap_dispatch_meta.set(meta);
        state.unwrap_dispatch_queue.insert(seq, request_id);
        state.unwrap_requests.insert(
            request_id,
            sample_unwrap_request(
                UnwrapRequestStatus::DispatchFailed,
                Some(super::UNWRAP_QUARANTINE_ERROR),
                1,
            ),
        );
    });

    let err = pop_next_dispatch_request(123).expect_err("must skip already quarantined entry");
    assert!(err.starts_with("wrap.dispatch.quarantined:"));
    let (stored, queue_len) = with_state(|state| {
        (
            state.unwrap_requests.get(&request_id),
            state.unwrap_dispatch_queue.len(),
        )
    });
    assert_eq!(queue_len, 0);
    assert_eq!(
        stored.and_then(|v| v.error_code),
        Some(super::UNWRAP_QUARANTINE_ERROR.to_string())
    );
}

#[test]
fn quarantine_decode_failed_unwrap_requests_marks_dead_letter_and_dequeues() {
    init_stable_state();
    let bad_request = TxId([0x35u8; 32]);
    let good_request = TxId([0x36u8; 32]);
    with_state_mut(|state| {
        let mut meta = *state.unwrap_dispatch_meta.get();
        let bad_seq = meta.push();
        let good_seq = meta.push();
        state.unwrap_dispatch_meta.set(meta);
        state.unwrap_dispatch_queue.insert(bad_seq, bad_request);
        state.unwrap_dispatch_queue.insert(good_seq, good_request);
        state.unwrap_requests.insert(
            bad_request,
            sample_unwrap_request(
                UnwrapRequestStatus::DispatchFailed,
                Some(super::UNWRAP_DECODE_FAILURE_CODE),
                1,
            ),
        );
        state.unwrap_requests.insert(
            good_request,
            UnwrapDispatchRequest {
                asset_id: vec![2u8; 5],
                amount: [3u8; 32],
                recipient: vec![4u8; 5],
                status: UnwrapRequestStatus::Queued,
                ledger_tx_id: None,
                error_code: None,
                updated_at: 1,
                transfer_created_at_time: 0,
            },
        );
    });

    let (quarantined, dropped) = super::quarantine_decode_failed_unwrap_requests(123);
    assert_eq!(quarantined, 1);
    assert_eq!(dropped, 1);
    let (quarantined_second, dropped_second) = super::quarantine_decode_failed_unwrap_requests(456);
    assert_eq!(quarantined_second, 0);
    assert_eq!(dropped_second, 0);
    let (bad, good, queue_len) = with_state(|state| {
        (
            state.unwrap_requests.get(&bad_request),
            state.unwrap_requests.get(&good_request),
            state.unwrap_dispatch_queue.len(),
        )
    });
    assert_eq!(queue_len, 1);
    assert_eq!(
        bad.as_ref().map(|v| v.status),
        Some(UnwrapRequestStatus::DispatchFailed)
    );
    assert_eq!(
        bad.and_then(|v| v.error_code),
        Some(super::UNWRAP_QUARANTINE_ERROR.to_string())
    );
    assert_eq!(
        good.as_ref().map(|v| v.status),
        Some(UnwrapRequestStatus::Queued)
    );
}

#[test]
fn did_contains_dispatch_result_contract_shape() {
    let did = include_str!("../evm_canister.did");
    assert!(did.contains("type UnwrapDispatchOverviewView = record {"));
    assert!(did.contains("type ApiError = variant {"));
    assert!(did.contains("type StandardRecord = record {"));
    assert!(did.contains("estimate_ic_tx"));
    assert!(did.contains("icrc10_supported_standards"));
    assert!(did.contains("icrc21_canister_call_consent_message"));
    assert!(did.contains("type RequestDispatchStatusView = variant {"));
    assert!(did.contains("wrap_canister_id : principal"));
    assert!(did.contains("wrap_factory_address : blob"));
    assert!(did.contains("wrap_config : opt WrapConfigArgs"));
    assert!(did.contains("type WrapConfigArgs = record {"));
    assert!(did.contains("get_fee_policy : () -> (Result_"));
    assert!(did.contains("get_allowed_assets : () -> (Result_"));
    assert!(did.contains("quote_wrap_request : (QuoteWrapRequestArgs) -> (Result_"));
    assert!(did.contains("quote_native_deposit : (QuoteNativeDepositArgs) -> (Result_"));
    assert!(did.contains("submit_wrap_request : (SubmitWrapRequestArgs) -> (Result_"));
    assert!(did.contains("submit_native_deposit : (SubmitNativeDepositArgs) -> (Result_"));
    assert!(did.contains("dispatch_unwrap_request : (DispatchUnwrapRequestArgs) -> (Result_"));
    assert!(did.contains("dispatch_native_withdrawal_request : ("));
    assert!(did.contains("DispatchNativeWithdrawalRequestArgs"));
    assert!(did.contains("get_request : (blob) -> (opt RequestOverview) query"));
    assert!(did.contains("get_native_deposit_result : (blob) -> (opt RequestOverview) query"));
    assert!(did.contains("retry_request : (RetryRequestArgs) -> (Result_"));
    assert!(did.contains("retry_native_deposit : (RetryRequestArgs) -> (Result_"));
    assert!(did.contains("retry_native_withdrawal : (RetryRequestArgs) -> (Result_"));
    assert!(did.contains("recover_failed_wrap : (RecoverFailedWrapArgs) -> (Result_"));
    assert!(did.contains("set_fee_policy : (FeePolicyView) -> (Result)"));
    assert!(did.contains("set_allowed_assets : (vec principal) -> (Result)"));
    assert!(!did.contains("get_request_dispatch_result"));
    assert!(did.contains("get_unwrap_request_ids_by_tx_id"));
    assert!(did.contains("get_unwrap_request_ids_by_eth_tx_hash"));
    assert!(did.contains("get_unwrap_dispatch_overview"));
    assert!(did.contains("DispatchUncertain"));
    assert!(did.contains("type IcpUpdateRequestView = record {"));
    assert!(did.contains("type IcpUpdateTxKindView = variant { EthSigned; IcSynthetic }"));
    assert!(did.contains("tx_kind : IcpUpdateTxKindView"));
    assert!(did.contains("evm_sender : blob"));
    assert!(did.contains("ic_caller : opt principal"));
    assert!(did.contains("get_icp_update_request : (blob) -> (opt IcpUpdateRequestView) query"));
    assert!(did.contains("get_update_precompile_allowlist : () -> (vec PrecompileAllowArgs) query"));
    assert!(did.contains("add_update_precompile_allowed_method : (PrecompileAllowArgs) -> (Result"));
    assert!(did.contains("remove_update_precompile_allowed_method : (PrecompileAllowArgs) -> ("));
    assert!(!did.contains("set_wrap_canister_id : (principal) -> (Result_15);"));
}
