//! どこで: Phase1.6 E2E / 何を: RPC互換メソッドをPocketICで確認 / なぜ: 実環境に近い互換確認のため

use candid::{Decode, Encode, Principal, CandidType};
use candid::Deserialize;
use evm_core::hash;
use pocket_ic::PocketIc;
use std::path::PathBuf;
use std::panic::{self, AssertUnwindSafe};
use std::time::Duration;

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum EthTxListView {
    Hashes(Vec<Vec<u8>>),
    Full(Vec<EthTxView>),
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct EthBlockView {
    number: u64,
    parent_hash: Vec<u8>,
    block_hash: Vec<u8>,
    timestamp: u64,
    txs: EthTxListView,
    state_root: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct EthTxView {
    hash: Vec<u8>,
    kind: EthTxKindView,
    raw: Vec<u8>,
    decoded: Option<DecodedTxView>,
    decode_ok: bool,
    block_number: Option<u64>,
    tx_index: Option<u32>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct DecodedTxView {
    from: Vec<u8>,
    to: Option<Vec<u8>>,
    nonce: u64,
    value: Vec<u8>,
    input: Vec<u8>,
    gas_limit: u64,
    gas_price: u128,
    chain_id: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum EthTxKindView {
    EthSigned,
    IcSynthetic,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct EthReceiptView {
    tx_hash: Vec<u8>,
    block_number: u64,
    tx_index: u32,
    status: u8,
    gas_used: u64,
    effective_gas_price: u64,
    contract_address: Option<Vec<u8>>,
    logs: Vec<EthReceiptLogView>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct EthReceiptLogView {
    address: Vec<u8>,
    topics: Vec<Vec<u8>>,
    data: Vec<u8>,
    log_index: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct GenesisBalanceView {
    address: Vec<u8>,
    amount: u128,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct InitArgs {
    genesis_balances: Vec<GenesisBalanceView>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum SubmitTxError {
    InvalidArgument(String),
    Rejected(String),
    Internal(String),
}

type SubmitTxResult = Result<Vec<u8>, SubmitTxError>;

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum ProduceBlockError {
    Internal(String),
    InvalidArgument(String),
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum NoOpReason {
    NoExecutableTx,
    CycleCritical,
    NeedsMigration,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum ProduceBlockStatus {
    Produced {
        block_number: u64,
        txs: u32,
        gas_used: u64,
        dropped: u32,
    },
    NoOp {
        reason: NoOpReason,
    },
}

type ProduceBlockResult = Result<ProduceBlockStatus, ProduceBlockError>;

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct PruneResultView {
    did_work: bool,
    remaining: u64,
    pruned_before_block: Option<u64>,
}

type PruneBlocksResult = Result<PruneResultView, ProduceBlockError>;
type ManageWriteResult = Result<(), String>;

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum PendingStatusView {
    Queued { seq: u64 },
    Included { block_number: u64, tx_index: u32 },
    Dropped { code: u16 },
    Unknown,
}

fn wasm_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("ic_evm_wrapper.wasm")
}

fn test_caller() -> Principal {
    Principal::self_authenticating(b"rpc-e2e-test-caller")
}

fn install_canister(pic: &PocketIc) -> Principal {
    let caller = test_caller();
    let init = Some(InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: hash::caller_evm_from_principal(caller.as_slice()).to_vec(),
            amount: 1_000_000_000_000_000_000u128,
        }],
    });
    let init_arg = Encode!(&init).expect("encode init args");
    let canister_id = install_canister_with_arg(pic, init_arg);
    pic.set_controllers(canister_id, Some(Principal::anonymous()), vec![caller])
        .unwrap_or_else(|err| panic!("set_controllers error: {err}"));
    settle_migrations(pic, canister_id, caller);
    canister_id
}

fn settle_migrations(pic: &PocketIc, _canister_id: Principal, _caller: Principal) {
    for _ in 0..6 {
        pic.advance_time(Duration::from_secs(60));
        pic.tick();
    }
}

fn install_canister_with_arg(pic: &PocketIc, init_arg: Vec<u8>) -> Principal {
    let path = wasm_path();
    if !path.exists() {
        panic!("wasm not found: build ic-evm-wrapper first: {:?}", path);
    }
    let wasm = std::fs::read(path).expect("read wasm");
    let canister_id = pic.create_canister();
    pic.add_cycles(canister_id, 5_000_000_000_000u128);
    pic.install_canister(canister_id, wasm, init_arg, None);
    canister_id
}

fn expect_install_trap(pic: &PocketIc, init_arg: Vec<u8>, expected: &str) {
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let _ = install_canister_with_arg(pic, init_arg);
    }));
    panic::set_hook(previous_hook);
    let payload = result.expect_err("install should fail");
    let message = panic_payload_message(payload);
    assert!(
        message.contains(expected),
        "unexpected trap message: {} (expected to contain {})",
        message,
        expected
    );
}

fn panic_payload_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(msg) = payload.downcast_ref::<String>() {
        return msg.clone();
    }
    if let Some(msg) = payload.downcast_ref::<&str>() {
        return (*msg).to_string();
    }
    "unknown panic payload".to_string()
}

fn call_query(pic: &PocketIc, canister_id: Principal, method: &str, arg: Vec<u8>) -> Vec<u8> {
    pic.query_call(canister_id, Principal::anonymous(), method, arg)
        .unwrap_or_else(|err| panic!("query error: {err}"))
}

fn call_update(pic: &PocketIc, canister_id: Principal, method: &str, arg: Vec<u8>) -> Vec<u8> {
    call_update_as(pic, canister_id, test_caller(), method, arg)
}

fn call_update_as(
    pic: &PocketIc,
    canister_id: Principal,
    caller: Principal,
    method: &str,
    arg: Vec<u8>,
) -> Vec<u8> {
    pic.update_call(canister_id, caller, method, arg)
        .unwrap_or_else(|err| panic!("update error: {err}"))
}

#[test]
fn rpc_chain_id_and_block_number_work() {
    let pic = PocketIc::new();
    let canister_id = install_canister(&pic);

    let arg = Encode!(&()).expect("encode");
    let chain_id_bytes = call_query(&pic, canister_id, "rpc_eth_chain_id", arg.clone());
    let block_number_bytes = call_query(&pic, canister_id, "rpc_eth_block_number", arg);

    let chain_id = Decode!(&chain_id_bytes, u64).expect("decode chain_id");
    let block_number = Decode!(&block_number_bytes, u64).expect("decode block_number");

    assert!(chain_id > 0);
    assert_eq!(block_number, 0);
}

#[test]
fn rpc_get_block_by_number_accepts_flags() {
    let pic = PocketIc::new();
    let canister_id = install_canister(&pic);

    let arg = Encode!(&0u64, &false).expect("encode");
    let block_bytes = call_query(&pic, canister_id, "rpc_eth_get_block_by_number", arg);
    let block: Option<EthBlockView> = Decode!(&block_bytes, Option<EthBlockView>)
        .expect("decode block");
    assert!(block.is_none());
}

#[test]
fn rpc_get_transaction_by_eth_hash_and_receipt_return_none() {
    let pic = PocketIc::new();
    let canister_id = install_canister(&pic);

    let tx_hash = vec![0u8; 32];
    let arg = Encode!(&tx_hash).expect("encode");
    let tx_bytes = call_query(&pic, canister_id, "rpc_eth_get_transaction_by_eth_hash", arg);
    let tx: Option<EthTxView> = Decode!(&tx_bytes, Option<EthTxView>).expect("decode tx");
    assert!(tx.is_none());

    let arg = Encode!(&tx_hash).expect("encode");
    let receipt_bytes = call_query(
        &pic,
        canister_id,
        "rpc_eth_get_transaction_receipt_by_eth_hash",
        arg,
    );
    let receipt: Option<EthReceiptView> =
        Decode!(&receipt_bytes, Option<EthReceiptView>).expect("decode receipt");
    assert!(receipt.is_none());
}

#[test]
fn rpc_get_transaction_receipt_by_eth_hash_returns_none_for_non_eth_tx() {
    let pic = PocketIc::new();
    let canister_id = install_canister(&pic);
    let tx_bytes = build_ic_tx_bytes([0x10u8; 20], 0);
    let mut tx_id: Option<Vec<u8>> = None;
    for _ in 0..4 {
        let submit_bytes = call_update(
            &pic,
            canister_id,
            "submit_ic_tx",
            Encode!(&tx_bytes).expect("encode submit"),
        );
        let submit: SubmitTxResult = Decode!(&submit_bytes, SubmitTxResult).expect("decode submit");
        match submit {
            Ok(value) => {
                tx_id = Some(value);
                break;
            }
            Err(SubmitTxError::Rejected(message)) if message == "ops.write.needs_migration" => {
                pic.advance_time(Duration::from_secs(60));
                pic.tick();
            }
            Err(other) => panic!("submit failed: {:?}", other),
        }
    }
    let tx_id = tx_id.expect("submit ok");
    let produce_bytes = call_update(
        &pic,
        canister_id,
        "produce_block",
        Encode!(&1u32).expect("encode produce"),
    );
    let produce: ProduceBlockResult = Decode!(&produce_bytes, ProduceBlockResult).expect("decode produce");
    match produce.expect("produce result") {
        ProduceBlockStatus::Produced { .. } => {}
        ProduceBlockStatus::NoOp { reason } => panic!("unexpected no-op: {:?}", reason),
    }

    let receipt_bytes = call_query(
        &pic,
        canister_id,
        "rpc_eth_get_transaction_receipt_by_eth_hash",
        Encode!(&tx_id).expect("encode receipt arg"),
    );
    let receipt: Option<EthReceiptView> =
        Decode!(&receipt_bytes, Option<EthReceiptView>).expect("decode receipt");
    assert!(receipt.is_none(), "ic synthetic tx has no eth_tx_hash");
}

#[test]
fn execute_ic_tx_is_removed_from_public_api() {
    let pic = PocketIc::new();
    let canister_id = install_canister(&pic);
    let result = pic.update_call(
        canister_id,
        Principal::anonymous(),
        "execute_ic_tx",
        Encode!(&Vec::<u8>::new()).expect("encode"),
    );
    assert!(result.is_err(), "execute_ic_tx should be undefined");
}

#[test]
fn prune_blocks_requires_controller() {
    let pic = PocketIc::new();
    let canister_id = install_canister(&pic);
    let non_controller = Principal::self_authenticating(b"non-controller");
    let out = pic
        .update_call(
            canister_id,
            non_controller,
            "prune_blocks",
            Encode!(&1u64, &1u32).expect("encode prune"),
        )
        .unwrap_or_else(|err| panic!("update error: {err}"));
    let result: PruneBlocksResult = Decode!(&out, PruneBlocksResult).expect("decode prune");
    match result {
        Ok(_) => panic!("anonymous caller must not prune"),
        Err(ProduceBlockError::Internal(message)) => {
            assert!(
                message.contains("auth.controller_required"),
                "unexpected message: {message}"
            );
        }
        Err(other) => panic!("unexpected prune error: {other:?}"),
    }
}

#[test]
fn unprivileged_produce_block_is_rejected() {
    let pic = PocketIc::new();
    let canister_id = install_canister(&pic);
    let unprivileged = Principal::self_authenticating(b"unprivileged-producer");
    let out = call_update_as(
        &pic,
        canister_id,
        unprivileged,
        "produce_block",
        Encode!(&1u32).expect("encode produce"),
    );
    let result: ProduceBlockResult = Decode!(&out, ProduceBlockResult).expect("decode produce");
    match result {
        Ok(_) => panic!("unprivileged produce_block must be rejected"),
        Err(ProduceBlockError::Internal(message)) => {
            assert!(
                message.contains("auth.controller_required")
                    || message.contains("ops.write.needs_migration"),
                "unexpected message: {message}"
            );
        }
        Err(other) => panic!("unexpected produce error: {other:?}"),
    }
}

#[test]
fn unprivileged_set_auto_mine_is_rejected() {
    let pic = PocketIc::new();
    let canister_id = install_canister(&pic);
    let unprivileged = Principal::self_authenticating(b"unprivileged-manager");
    let out = call_update_as(
        &pic,
        canister_id,
        unprivileged,
        "set_auto_mine",
        Encode!(&true).expect("encode set_auto_mine"),
    );
    let result: ManageWriteResult = Decode!(&out, ManageWriteResult).expect("decode set_auto_mine");
    match result {
        Ok(()) => panic!("unprivileged set_auto_mine must be rejected"),
        Err(message) => {
            assert!(
                message.contains("auth.controller_required"),
                "unexpected message: {message}"
            );
        }
    }
}

#[test]
#[ignore = "manual_pocket_ic_timing_sensitive"]
fn instruction_soft_limit_blocks_inclusion_in_pending_status() {
    let pic = PocketIc::new();
    let canister_id = install_canister(&pic);

    let set_limit_out = call_update(
        &pic,
        canister_id,
        "set_instruction_soft_limit",
        Encode!(&1u64).expect("encode limit"),
    );
    let set_limit: ManageWriteResult =
        Decode!(&set_limit_out, ManageWriteResult).expect("decode set_instruction_soft_limit");
    assert_eq!(set_limit, Ok(()));

    let tx_bytes = build_ic_tx_bytes([0x22u8; 20], 0);
    let submit_out = call_update(
        &pic,
        canister_id,
        "submit_ic_tx",
        Encode!(&tx_bytes).expect("encode submit"),
    );
    let submit: SubmitTxResult = Decode!(&submit_out, SubmitTxResult).expect("decode submit");
    let tx_id = match submit {
        Ok(value) => value,
        Err(err) => panic!("submit failed: {:?}", err),
    };

    let produce_out = call_update(
        &pic,
        canister_id,
        "produce_block",
        Encode!(&1u32).expect("encode produce"),
    );
    let produce: ProduceBlockResult = Decode!(&produce_out, ProduceBlockResult).expect("decode produce");
    match produce.expect("produce result") {
        ProduceBlockStatus::NoOp {
            reason: NoOpReason::NoExecutableTx,
        } => {}
        other => panic!("unexpected produce status: {:?}", other),
    }

    let pending_out = call_query(
        &pic,
        canister_id,
        "get_pending",
        Encode!(&tx_id).expect("encode get_pending"),
    );
    let pending: PendingStatusView =
        Decode!(&pending_out, PendingStatusView).expect("decode get_pending");
    match pending {
        PendingStatusView::Queued { .. } | PendingStatusView::Dropped { code: 9 } => {}
        other => panic!(
            "expected tx to be non-included under tight instruction limit, got {:?}",
            other
        ),
    }
}

#[test]
fn install_rejects_none_init_args() {
    let pic = PocketIc::new();
    let init_arg = Encode!(&Option::<InitArgs>::None).expect("encode none init args");
    expect_install_trap(
        &pic,
        init_arg,
        "InitArgsRequired: InitArgs is required; pass (opt record {...})",
    );
}

#[test]
fn install_rejects_invalid_init_args() {
    let pic = PocketIc::new();
    let bad_address = Some(InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: vec![0u8; 19],
            amount: 1,
        }],
    });
    let bad_address_arg = Encode!(&bad_address).expect("encode bad address init args");
    expect_install_trap(
        &pic,
        bad_address_arg,
        "InvalidInitArgs: balance[0].address must be 20 bytes",
    );

    let zero_amount = Some(InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: vec![0u8; 20],
            amount: 0,
        }],
    });
    let zero_amount_arg = Encode!(&zero_amount).expect("encode zero amount init args");
    expect_install_trap(
        &pic,
        zero_amount_arg,
        "InvalidInitArgs: balance[0].amount must be > 0",
    );

    let duplicate = Some(InitArgs {
        genesis_balances: vec![
            GenesisBalanceView {
                address: vec![0x11u8; 20],
                amount: 1,
            },
            GenesisBalanceView {
                address: vec![0x11u8; 20],
                amount: 2,
            },
        ],
    });
    let duplicate_arg = Encode!(&duplicate).expect("encode duplicate init args");
    expect_install_trap(
        &pic,
        duplicate_arg,
        "InvalidInitArgs: duplicate genesis address at balance[1]",
    );
}

fn build_ic_tx_bytes(to: [u8; 20], nonce: u64) -> Vec<u8> {
    let value = [0u8; 32];
    let gas_limit = 50_000u64.to_be_bytes();
    let nonce = nonce.to_be_bytes();
    let max_fee = 2_000_000_000u128.to_be_bytes();
    let max_priority = 1_000_000_000u128.to_be_bytes();
    let data: Vec<u8> = Vec::new();
    let data_len = u32::try_from(data.len()).unwrap_or(0).to_be_bytes();
    let mut out = Vec::with_capacity(1 + 20 + 32 + 8 + 8 + 16 + 16 + 4 + data.len());
    out.push(2u8);
    out.extend_from_slice(&to);
    out.extend_from_slice(&value);
    out.extend_from_slice(&gas_limit);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&max_fee);
    out.extend_from_slice(&max_priority);
    out.extend_from_slice(&data_len);
    out.extend_from_slice(&data);
    out
}
