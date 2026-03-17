//! どこで: Phase1.6 E2E / 何を: RPC互換メソッドをPocketICで確認 / なぜ: 実環境に近い互換確認のため

use candid::Deserialize;
use candid::{CandidType, Decode, Encode, Principal};
use evm_core::hash;
use pocket_ic::PocketIc;
use serde_json::Value;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::time::Duration;
use tiny_keccak::{Hasher, Keccak};

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
    gas_price: Option<u128>,
    max_fee_per_gas: Option<u128>,
    max_priority_fee_per_gas: Option<u128>,
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
    wrap_canister_id: Principal,
    wrap_factory_address: Vec<u8>,
    query_instruction_soft_limit: Option<u64>,
    update_instruction_soft_limit: Option<u64>,
}

const TEST_WRAP_FACTORY_ADDRESS: [u8; 20] = [0x90u8; 20];
const TEST_FACTORY_TRACE_GENESIS_BALANCE_WEI: u128 = 10_000_000_000_000_000_000_000_000u128;

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
struct PruneResultView {
    did_work: bool,
    remaining: u64,
    pruned_before_block: Option<u64>,
}

type PruneBlocksResult = Result<PruneResultView, ProduceBlockError>;
#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum PendingStatusView {
    Queued { seq: u64 },
    Included { block_number: u64, tx_index: u32 },
    Dropped { code: u16 },
    Unknown,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct ExportCursorView {
    block_number: u64,
    segment: u8,
    byte_offset: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct ExportChunkView {
    segment: u8,
    start: u32,
    bytes: Vec<u8>,
    payload_len: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct ExportResponseView {
    chunks: Vec<ExportChunkView>,
    next_cursor: Option<ExportCursorView>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum ExportErrorView {
    InvalidCursor { message: String },
    Pruned { pruned_before_block: u64 },
    MissingData { message: String },
    Limit,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct ReceiptView {
    tx_id: Vec<u8>,
    block_number: u64,
    tx_index: u32,
    status: u8,
    gas_used: u64,
    effective_gas_price: u64,
    l1_data_fee: u128,
    operator_fee: u128,
    total_fee: u128,
    return_data_hash: Vec<u8>,
    return_data: Option<Vec<u8>>,
    contract_address: Option<Vec<u8>>,
    logs: Vec<LogView>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum LookupError {
    NotFound,
    Pruned { pruned_before_block: u64 },
    Pending,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct LogView {
    address: Vec<u8>,
    topics: Vec<Vec<u8>>,
    data: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct SubmitIcTxArgsDto {
    to: Option<Vec<u8>>,
    from: Option<Vec<u8>>,
    value: candid::Nat,
    max_priority_fee_per_gas: candid::Nat,
    data: Vec<u8>,
    max_fee_per_gas: candid::Nat,
    nonce: u64,
    gas_limit: u64,
}

fn wasm_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("ic_evm_gateway.wasm")
}

fn test_caller() -> Principal {
    Principal::self_authenticating(b"rpc-e2e-test-caller")
}

fn install_canister(pic: &PocketIc) -> Principal {
    let caller = test_caller();
    let wrap_canister_id = pic.create_canister();
    let init = Some(InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: hash::derive_evm_address_from_principal(caller.as_slice())
                .expect("must derive")
                .to_vec(),
            amount: 1_000_000_000_000_000_000u128,
        }],
        wrap_canister_id,
        wrap_factory_address: TEST_WRAP_FACTORY_ADDRESS.to_vec(),
        query_instruction_soft_limit: None,
        update_instruction_soft_limit: None,
    });
    let init_arg = Encode!(&init).expect("encode init args");
    pic.add_cycles(wrap_canister_id, 5_000_000_000_000u128);
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
        panic!("wasm not found: build ic-evm-gateway first: {:?}", path);
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

fn install_canister_for_factory_trace(pic: &PocketIc) -> Principal {
    let caller = test_caller();
    let caller_evm = hash::derive_evm_address_from_principal(caller.as_slice())
        .expect("must derive caller evm address");
    let wrap_canister_id = pic.create_canister();
    let init = Some(InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: caller_evm.to_vec(),
            amount: TEST_FACTORY_TRACE_GENESIS_BALANCE_WEI,
        }],
        wrap_canister_id,
        wrap_factory_address: predict_create_address(caller_evm, 0).to_vec(),
        query_instruction_soft_limit: None,
        update_instruction_soft_limit: None,
    });
    let init_arg = Encode!(&init).expect("encode init args");
    pic.add_cycles(wrap_canister_id, 5_000_000_000_000u128);
    let canister_id = install_canister_with_arg(pic, init_arg);
    pic.set_controllers(canister_id, Some(Principal::anonymous()), vec![caller])
        .unwrap_or_else(|err| panic!("set_controllers error: {err}"));
    settle_migrations(pic, canister_id, caller);
    canister_id
}

fn abi_word_from_u128(value: u128) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[16..].copy_from_slice(&value.to_be_bytes());
    out
}

fn abi_word_from_u64(value: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[24..].copy_from_slice(&value.to_be_bytes());
    out
}

fn abi_word_from_u8(value: u8) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[31] = value;
    out
}

fn abi_word_from_address(address: [u8; 20]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[12..].copy_from_slice(&address);
    out
}

fn function_selector(signature: &str) -> [u8; 4] {
    let mut hasher = Keccak::v256();
    hasher.update(signature.as_bytes());
    let mut out = [0u8; 32];
    hasher.finalize(&mut out);
    let mut selector = [0u8; 4];
    selector.copy_from_slice(&out[..4]);
    selector
}

fn predict_create_address(sender: [u8; 20], nonce: u64) -> [u8; 20] {
    assert_eq!(nonce, 0, "test helper only supports nonce 0");
    let mut payload = Vec::with_capacity(23);
    payload.push(0xd6);
    payload.push(0x94);
    payload.extend_from_slice(&sender);
    payload.push(0x80);
    let mut hasher = Keccak::v256();
    hasher.update(&payload);
    let mut out = [0u8; 32];
    hasher.finalize(&mut out);
    let mut address = [0u8; 20];
    address.copy_from_slice(&out[12..]);
    address
}

fn wrap_factory_artifact_bytecode() -> Vec<u8> {
    let artifact = include_str!(
        "../../../tools/wrapper/contracts/out/WrapTokenFactory.sol/WrapTokenFactory.json"
    );
    let value: Value = serde_json::from_str(artifact).expect("parse factory artifact json");
    let object = value["bytecode"]["object"]
        .as_str()
        .expect("factory bytecode object");
    hex::decode(object.trim_start_matches("0x")).expect("decode factory bytecode")
}

fn encode_constructor_address(address: [u8; 20]) -> Vec<u8> {
    abi_word_from_address(address).to_vec()
}

fn encode_mint_for_asset(
    canister_id: Principal,
    decimals: u8,
    to: [u8; 20],
    amount: u128,
) -> Vec<u8> {
    let canister_bytes = canister_id.as_slice();
    let tail_len = 32 + ((canister_bytes.len() + 31) / 32) * 32;
    let mut out = Vec::with_capacity(4 + 128 + tail_len);
    out.extend_from_slice(&function_selector(
        "mintForAsset(bytes,uint8,address,uint256)",
    ));
    out.extend_from_slice(&abi_word_from_u64(128));
    out.extend_from_slice(&abi_word_from_u8(decimals));
    out.extend_from_slice(&abi_word_from_address(to));
    out.extend_from_slice(&abi_word_from_u128(amount));
    out.extend_from_slice(&abi_word_from_u64(canister_bytes.len() as u64));
    let mut padded = vec![0u8; tail_len - 32];
    padded[..canister_bytes.len()].copy_from_slice(canister_bytes);
    out.extend_from_slice(&padded);
    out
}

fn call_get_receipt(
    pic: &PocketIc,
    canister_id: Principal,
    tx_id: &[u8],
) -> Result<ReceiptView, LookupError> {
    let out = call_query(
        pic,
        canister_id,
        "get_receipt",
        Encode!(&tx_id.to_vec()).expect("encode receipt query"),
    );
    Decode!(&out, Result<ReceiptView, LookupError>).expect("decode receipt query")
}

fn wait_for_receipt(pic: &PocketIc, canister_id: Principal, tx_id: &[u8]) -> ReceiptView {
    for _ in 0..12 {
        pic.advance_time(Duration::from_secs(60));
        pic.tick();
        match call_get_receipt(pic, canister_id, tx_id) {
            Ok(receipt) => return receipt,
            Err(LookupError::Pending | LookupError::NotFound) => {}
            Err(err) => panic!("unexpected receipt lookup error: {err:?}"),
        }
    }
    panic!("receipt did not materialize");
}

fn call_export_blocks(
    pic: &PocketIc,
    canister_id: Principal,
    cursor: Option<ExportCursorView>,
    max_bytes: u32,
) -> Result<ExportResponseView, ExportErrorView> {
    let out = call_query(
        pic,
        canister_id,
        "export_blocks",
        Encode!(&cursor, &max_bytes).expect("encode export_blocks args"),
    );
    Decode!(&out, Result<ExportResponseView, ExportErrorView>).expect("decode export_blocks")
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
    let block: Option<EthBlockView> =
        Decode!(&block_bytes, Option<EthBlockView>).expect("decode block");
    assert!(block.is_none());
}

#[test]
fn rpc_get_transaction_by_eth_hash_and_receipt_return_none() {
    let pic = PocketIc::new();
    let canister_id = install_canister(&pic);

    let tx_hash = vec![0u8; 32];
    let arg = Encode!(&tx_hash).expect("encode");
    let tx_bytes = call_query(
        &pic,
        canister_id,
        "rpc_eth_get_transaction_by_eth_hash",
        arg,
    );
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
    let submit_args = build_submit_ic_tx_args([0x10u8; 20], 0);
    let mut tx_id: Option<Vec<u8>> = None;
    for _ in 0..4 {
        let submit_bytes = call_update(
            &pic,
            canister_id,
            "submit_ic_tx",
            Encode!(&submit_args).expect("encode submit"),
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
#[ignore = "manual_pocket_ic_timing_sensitive"]
fn query_instruction_soft_limit_blocks_inclusion_in_pending_status() {
    let pic = PocketIc::new();
    let canister_id = install_canister(&pic);
    let caller = test_caller();
    let wrap_canister_id = pic.create_canister();
    pic.add_cycles(wrap_canister_id, 5_000_000_000_000u128);
    let upgrade_args = Some(InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: hash::derive_evm_address_from_principal(caller.as_slice())
                .expect("must derive")
                .to_vec(),
            amount: 1_000_000_000_000_000_000u128,
        }],
        wrap_canister_id,
        wrap_factory_address: TEST_WRAP_FACTORY_ADDRESS.to_vec(),
        query_instruction_soft_limit: Some(1),
        update_instruction_soft_limit: None,
    });
    let wasm = std::fs::read(wasm_path()).expect("read wasm");
    pic.upgrade_canister(
        canister_id,
        wasm,
        Encode!(&upgrade_args).expect("encode upgrade args"),
        None,
    )
    .expect("upgrade canister");

    let submit_args = build_submit_ic_tx_args([0x22u8; 20], 0);
    let submit_out = call_update(
        &pic,
        canister_id,
        "submit_ic_tx",
        Encode!(&submit_args).expect("encode submit"),
    );
    let submit: SubmitTxResult = Decode!(&submit_out, SubmitTxResult).expect("decode submit");
    let tx_id = match submit {
        Ok(value) => value,
        Err(err) => panic!("submit failed: {:?}", err),
    };

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
        wrap_canister_id: Principal::self_authenticating(b"wrap"),
        wrap_factory_address: TEST_WRAP_FACTORY_ADDRESS.to_vec(),
        query_instruction_soft_limit: None,
        update_instruction_soft_limit: None,
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
        wrap_canister_id: Principal::self_authenticating(b"wrap"),
        wrap_factory_address: TEST_WRAP_FACTORY_ADDRESS.to_vec(),
        query_instruction_soft_limit: None,
        update_instruction_soft_limit: None,
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
        wrap_canister_id: Principal::self_authenticating(b"wrap"),
        wrap_factory_address: TEST_WRAP_FACTORY_ADDRESS.to_vec(),
        query_instruction_soft_limit: None,
        update_instruction_soft_limit: None,
    });
    let duplicate_arg = Encode!(&duplicate).expect("encode duplicate init args");
    expect_install_trap(
        &pic,
        duplicate_arg,
        "InvalidInitArgs: duplicate genesis address at balance[1]",
    );

    let bad_factory = Some(InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: vec![0u8; 20],
            amount: 1,
        }],
        wrap_canister_id: Principal::self_authenticating(b"wrap"),
        wrap_factory_address: vec![0u8; 19],
        query_instruction_soft_limit: None,
        update_instruction_soft_limit: None,
    });
    let bad_factory_arg = Encode!(&bad_factory).expect("encode bad factory init args");
    expect_install_trap(
        &pic,
        bad_factory_arg,
        "InvalidInitArgs: wrap_factory_address must be 20 bytes",
    );
}

#[test]
fn export_blocks_exposes_internal_trace_segment_for_factory_mint() {
    let pic = PocketIc::new();
    let canister_id = install_canister_for_factory_trace(&pic);
    let caller_evm = hash::derive_evm_address_from_principal(test_caller().as_slice())
        .expect("derive caller evm address");
    let factory = predict_create_address(caller_evm, 0);

    let mut deploy_data = wrap_factory_artifact_bytecode();
    deploy_data.extend_from_slice(&encode_constructor_address(caller_evm));
    let deploy_out = call_update(
        &pic,
        canister_id,
        "submit_ic_tx",
        Encode!(&build_submit_ic_contract_tx_args(
            None,
            0,
            deploy_data,
            8_000_000,
        ))
        .expect("encode deploy"),
    );
    let deploy_result: SubmitTxResult =
        Decode!(&deploy_out, SubmitTxResult).expect("decode deploy");
    let deploy_tx_id = deploy_result.expect("deploy tx ok");
    let deploy_receipt = wait_for_receipt(&pic, canister_id, &deploy_tx_id);
    assert_eq!(deploy_receipt.status, 1, "factory deploy failed");
    assert_eq!(deploy_receipt.contract_address, Some(factory.to_vec()));

    let mint_data = encode_mint_for_asset(
        Principal::self_authenticating(b"trace-export-asset"),
        8,
        caller_evm,
        1_000_000_000_000u128,
    );
    let mint_out = call_update(
        &pic,
        canister_id,
        "submit_ic_tx",
        Encode!(&build_submit_ic_contract_tx_args(
            Some(factory),
            1,
            mint_data,
            3_000_000,
        ))
        .expect("encode mint"),
    );
    let mint_result: SubmitTxResult = Decode!(&mint_out, SubmitTxResult).expect("decode mint");
    let mint_tx_id = mint_result.expect("mint tx ok");
    let mint_receipt = wait_for_receipt(&pic, canister_id, &mint_tx_id);
    assert_eq!(mint_receipt.status, 1, "mint tx failed");

    let export = call_export_blocks(
        &pic,
        canister_id,
        Some(ExportCursorView {
            block_number: mint_receipt.block_number,
            segment: 3,
            byte_offset: 0,
        }),
        1_500_000,
    )
    .expect("export_blocks should succeed");
    let internal_trace_bytes = export
        .chunks
        .iter()
        .take_while(|chunk| chunk.segment == 3)
        .flat_map(|chunk| chunk.bytes.iter().copied())
        .collect::<Vec<_>>();

    assert!(
        !internal_trace_bytes.is_empty(),
        "segment 3 should include internal trace payload"
    );
    assert_eq!(&internal_trace_bytes[..32], mint_tx_id.as_slice());

    let payload_len = u32::from_be_bytes(
        internal_trace_bytes[32..36]
            .try_into()
            .expect("payload_len bytes"),
    ) as usize;
    let payload = &internal_trace_bytes[36..36 + payload_len];
    assert_eq!(payload[0], 3, "internal trace payload version changed");
    assert_eq!(payload[1], 0, "mint trace should not be encode_failed");
    let truncated = payload[2];
    let captured_count =
        u32::from_be_bytes(payload[3..7].try_into().expect("captured_count bytes"));
    let total_count = u32::from_be_bytes(payload[7..11].try_into().expect("total_count bytes"));
    let row_count = u32::from_be_bytes(payload[11..15].try_into().expect("row_count bytes"));
    assert_eq!(truncated, 0, "mint trace should fit export limit in E2E");
    assert!(
        captured_count > 0,
        "mint should emit at least one internal trace"
    );
    assert_eq!(
        captured_count, row_count,
        "non-truncated export should be lossless"
    );
    assert!(
        total_count >= captured_count,
        "trace metadata must be monotonic"
    );
}

fn build_submit_ic_tx_args(to: [u8; 20], nonce: u64) -> SubmitIcTxArgsDto {
    build_submit_ic_contract_tx_args(Some(to), nonce, Vec::new(), 50_000)
}

fn build_submit_ic_contract_tx_args(
    to: Option<[u8; 20]>,
    nonce: u64,
    data: Vec<u8>,
    gas_limit: u64,
) -> SubmitIcTxArgsDto {
    // NOTE: keep test txs valid under current fee floor policy.
    // max_priority(300 gwei) >= min_priority(250 gwei),
    // max_fee(600 gwei) >= base_fee + min_priority (500 gwei).
    SubmitIcTxArgsDto {
        to: to.map(|value| value.to_vec()),
        from: None,
        value: candid::Nat::from(0u8),
        max_priority_fee_per_gas: candid::Nat::from(300_000_000_000u64),
        data,
        max_fee_per_gas: candid::Nat::from(600_000_000_000u64),
        nonce,
        gas_limit,
    }
}
