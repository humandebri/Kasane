//! どこで: unwrap dispatch の PocketIC E2E
//! 何を: upgrade 後の unwrap 再送が idempotent に復旧することを確認
//! なぜ: gateway の Dispatching 再開と wrap 側 idempotency の回帰を防ぐため

use candid::{CandidType, Decode, Deserialize, Encode, Nat, Principal};
use evm_core::hash;
use evm_core::wrap_precompile::WRAP_PRECOMPILE_ADDRESS;
use pocket_ic::PocketIc;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tiny_keccak::{Hasher, Keccak};

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct GenesisBalanceView {
    address: Vec<u8>,
    amount: u128,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct GatewayInitArgs {
    genesis_balances: Vec<GenesisBalanceView>,
    wrap_canister_id: Principal,
    wrap_factory_address: Vec<u8>,
}

const WRAP_AMOUNT_E8S: u128 = 1_000_000_000_000u128;
const TEST_ASSET_DECIMALS: u8 = 8;
const TEST_GENESIS_BALANCE_WEI: u128 = 10_000_000_000_000_000_000_000_000u128;
const TEST_WRAP_GAS_LIMIT: u64 = 3_000_000;

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

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum SubmitTxError {
    InvalidArgument(String),
    Rejected(String),
    Internal(String),
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum PendingStatusView {
    Queued { seq: u64 },
    Included { block_number: u64, tx_index: u32 },
    Dropped { code: u16 },
    Unknown,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum RequestDispatchStatusView {
    Queued,
    Dispatching,
    Dispatched,
    DispatchFailed,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct UnwrapDispatchOverviewView {
    request_id: Vec<u8>,
    status: RequestDispatchStatusView,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum WrapRequestStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct DispatchUnwrapRequestArgs {
    request_id: Vec<u8>,
    asset_id: Principal,
    amount_e8s: Nat,
    recipient: Principal,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct DispatchUnwrapRequestOk {
    request_id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct ApiErrorDetail {
    code: String,
    message: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum ApiError {
    InvalidArgument(ApiErrorDetail),
    Rejected(ApiErrorDetail),
    Internal(ApiErrorDetail),
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum RequestKind {
    Unwrap,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct RequestOverview {
    kind: RequestKind,
    request_id: Vec<u8>,
    status: WrapRequestStatus,
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
struct LogView {
    address: Vec<u8>,
    topics: Vec<Vec<u8>>,
    data: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum LookupError {
    NotFound,
    Pruned { pruned_before_block: u64 },
    Pending,
}

fn gateway_wasm_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("ic_evm_gateway.wasm")
}

fn wrap_wasm_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("mock_wrap_canister.wasm")
}

fn test_caller() -> Principal {
    Principal::self_authenticating(b"unwrap-recovery-e2e-caller")
}

fn read_wasm(path: PathBuf) -> Vec<u8> {
    if !path.exists() {
        panic!("wasm not found: build release wasm first: {path:?}");
    }
    std::fs::read(path).expect("read wasm")
}

fn install_pair(pic: &PocketIc) -> (Principal, Principal) {
    let gateway_id = pic.create_canister();
    let wrap_id = pic.create_canister();
    pic.add_cycles(gateway_id, 5_000_000_000_000u128);
    pic.add_cycles(wrap_id, 5_000_000_000_000u128);

    let caller_evm = hash::derive_evm_address_from_principal(test_caller().as_slice())
        .expect("derive caller evm address");
    let gateway_init = Some(GatewayInitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: caller_evm.to_vec(),
            amount: TEST_GENESIS_BALANCE_WEI,
        }],
        wrap_canister_id: wrap_id,
        wrap_factory_address: predict_create_address(caller_evm, 0).to_vec(),
    });
    pic.install_canister(
        wrap_id,
        read_wasm(wrap_wasm_path()),
        Encode!(&()).expect("encode mock wrap init"),
        None,
    );
    pic.install_canister(
        gateway_id,
        read_wasm(gateway_wasm_path()),
        Encode!(&gateway_init).expect("encode gateway init"),
        None,
    );
    pic.set_controllers(
        gateway_id,
        Some(Principal::anonymous()),
        vec![test_caller()],
    )
    .unwrap_or_else(|err| panic!("set gateway controllers failed: {err}"));

    settle_gateway(pic);
    (gateway_id, wrap_id)
}

fn settle_gateway(pic: &PocketIc) {
    for _ in 0..6 {
        pic.advance_time(Duration::from_secs(60));
        pic.tick();
    }
}

fn encode_unwrap_payload(asset: Principal, recipient: Principal) -> Vec<u8> {
    fn principal_field(principal: Principal) -> Vec<u8> {
        let bytes = principal.as_slice();
        let mut out = vec![0u8; 30];
        out[0] = bytes.len() as u8;
        out[1..1 + bytes.len()].copy_from_slice(bytes);
        out
    }

    let mut amount = [0u8; 32];
    amount[16..].copy_from_slice(&1_000_000_000_000u128.to_be_bytes());
    let mut out = Vec::with_capacity(93);
    out.push(1);
    out.extend_from_slice(&principal_field(asset));
    out.extend_from_slice(&amount);
    out.extend_from_slice(&principal_field(recipient));
    out
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

fn encode_approve(factory: [u8; 20], amount: u128) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&function_selector("approve(address,uint256)"));
    out.extend_from_slice(&abi_word_from_address(factory));
    out.extend_from_slice(&abi_word_from_u128(amount));
    out
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

fn build_submit_ic_tx_args(
    to: Option<[u8; 20]>,
    nonce: u64,
    data: Vec<u8>,
    gas_limit: u64,
) -> SubmitIcTxArgsDto {
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

fn submit_unwrap_tx(pic: &PocketIc, gateway_id: Principal, data: Vec<u8>) -> Vec<u8> {
    let out = pic
        .update_call(
            gateway_id,
            test_caller(),
            "submit_ic_tx",
            Encode!(&build_submit_ic_tx_args(
                Some(WRAP_PRECOMPILE_ADDRESS.into_array()),
                3,
                data,
                300_000,
            ))
            .expect("encode submit"),
        )
        .unwrap_or_else(|err| panic!("submit update failed: {err}"));
    let result: Result<Vec<u8>, SubmitTxError> =
        Decode!(&out, Result<Vec<u8>, SubmitTxError>).expect("decode submit result");
    result.unwrap_or_else(|err| panic!("submit failed: {err:?}"))
}

fn gateway_receipt(
    pic: &PocketIc,
    gateway_id: Principal,
    tx_id: &[u8],
) -> Result<ReceiptView, LookupError> {
    let out = pic
        .query_call(
            gateway_id,
            Principal::anonymous(),
            "get_receipt",
            Encode!(&tx_id.to_vec()).expect("encode receipt query"),
        )
        .unwrap();
    Decode!(&out, Result<ReceiptView, LookupError>).expect("decode receipt")
}

fn gateway_pending_status(
    pic: &PocketIc,
    gateway_id: Principal,
    tx_id: &[u8],
) -> PendingStatusView {
    let out = pic
        .query_call(
            gateway_id,
            Principal::anonymous(),
            "get_pending",
            Encode!(&tx_id.to_vec()).expect("encode pending query"),
        )
        .unwrap();
    Decode!(&out, PendingStatusView).expect("decode pending status")
}

fn wait_for_receipt(pic: &PocketIc, gateway_id: Principal, tx_id: &[u8]) -> ReceiptView {
    let mut last_pending = None;
    for _ in 0..12 {
        pic.advance_time(Duration::from_secs(60));
        pic.tick();
        last_pending = Some(gateway_pending_status(pic, gateway_id, tx_id));
        match gateway_receipt(pic, gateway_id, tx_id) {
            Ok(receipt) => return receipt,
            Err(LookupError::Pending | LookupError::NotFound) => {}
            Err(err) => panic!("unexpected receipt lookup error: {err:?}"),
        }
    }
    panic!("receipt did not materialize; pending={last_pending:?}");
}

fn setup_unwrap_state(pic: &PocketIc, gateway_id: Principal, asset: Principal) {
    let caller_evm = hash::derive_evm_address_from_principal(test_caller().as_slice())
        .expect("derive caller evm");
    let factory = predict_create_address(caller_evm, 0);

    let mut deploy_data = wrap_factory_artifact_bytecode();
    deploy_data.extend_from_slice(&encode_constructor_address(caller_evm));
    let deploy_out = pic
        .update_call(
            gateway_id,
            test_caller(),
            "submit_ic_tx",
            Encode!(&build_submit_ic_tx_args(None, 0, deploy_data, 8_000_000))
                .expect("encode deploy"),
        )
        .unwrap();
    let deploy_result: Result<Vec<u8>, SubmitTxError> =
        Decode!(&deploy_out, Result<Vec<u8>, SubmitTxError>).expect("decode deploy");
    let deploy_tx_id = deploy_result.expect("deploy tx ok");
    let deploy_receipt = wait_for_receipt(pic, gateway_id, &deploy_tx_id);
    assert_eq!(deploy_receipt.status, 1, "factory deploy failed");
    assert_eq!(deploy_receipt.contract_address, Some(factory.to_vec()));

    let mint_data = encode_mint_for_asset(asset, TEST_ASSET_DECIMALS, caller_evm, WRAP_AMOUNT_E8S);
    let mint_out = pic
        .update_call(
            gateway_id,
            test_caller(),
            "submit_ic_tx",
            Encode!(&build_submit_ic_tx_args(
                Some(factory),
                1,
                mint_data,
                TEST_WRAP_GAS_LIMIT,
            ))
            .expect("encode mint"),
        )
        .unwrap();
    let mint_result: Result<Vec<u8>, SubmitTxError> =
        Decode!(&mint_out, Result<Vec<u8>, SubmitTxError>).expect("decode mint");
    let mint_tx_id = mint_result.expect("mint tx ok");
    let mint_receipt = wait_for_receipt(pic, gateway_id, &mint_tx_id);
    assert_eq!(mint_receipt.status, 1, "mint tx failed");
    let token = mint_receipt
        .return_data
        .as_ref()
        .and_then(|data| data.get(data.len().saturating_sub(20)..))
        .expect("mint return_data should contain token address")
        .to_vec();

    let approve_data = encode_approve(factory, WRAP_AMOUNT_E8S);
    let approve_out = pic
        .update_call(
            gateway_id,
            test_caller(),
            "submit_ic_tx",
            Encode!(&build_submit_ic_tx_args(
                Some({
                    let mut address = [0u8; 20];
                    address.copy_from_slice(&token);
                    address
                }),
                2,
                approve_data,
                120_000,
            ))
            .expect("encode approve"),
        )
        .unwrap();
    let approve_result: Result<Vec<u8>, SubmitTxError> =
        Decode!(&approve_out, Result<Vec<u8>, SubmitTxError>).expect("decode approve");
    let approve_tx_id = approve_result.expect("approve tx ok");
    let approve_receipt = wait_for_receipt(pic, gateway_id, &approve_tx_id);
    assert_eq!(approve_receipt.status, 1, "approve tx failed");
}

fn seed_wrap_request(
    pic: &PocketIc,
    wrap_id: Principal,
    request_id: &[u8],
    asset_id: Principal,
    recipient: Principal,
) {
    let out = pic
        .update_call(
            wrap_id,
            Principal::anonymous(),
            "dispatch_unwrap_request",
            Encode!(&DispatchUnwrapRequestArgs {
                request_id: request_id.to_vec(),
                asset_id,
                amount_e8s: Nat::from(1_000_000_000_000u128),
                recipient,
            })
            .expect("encode mock wrap submit"),
        )
        .unwrap_or_else(|err| panic!("seed wrap update failed: {err}"));
    let result: Result<DispatchUnwrapRequestOk, ApiError> =
        Decode!(&out, Result<DispatchUnwrapRequestOk, ApiError>)
            .expect("decode mock wrap seed result");
    assert!(result.is_ok(), "seed wrap request failed: {result:?}");
}

fn gateway_dispatch_result(
    pic: &PocketIc,
    gateway_id: Principal,
    request_id: &[u8],
) -> Option<UnwrapDispatchOverviewView> {
    let out = pic
        .query_call(
            gateway_id,
            Principal::anonymous(),
            "get_unwrap_dispatch_overview",
            Encode!(&request_id.to_vec()).expect("encode result query"),
        )
        .unwrap_or_else(|err| panic!("gateway result query failed: {err}"));
    Decode!(&out, Option<UnwrapDispatchOverviewView>).expect("decode gateway result")
}

fn gateway_unwrap_request_ids_by_tx_id(
    pic: &PocketIc,
    gateway_id: Principal,
    tx_id: &[u8],
) -> Vec<Vec<u8>> {
    let out = pic
        .query_call(
            gateway_id,
            Principal::anonymous(),
            "get_unwrap_request_ids_by_tx_id",
            Encode!(&tx_id.to_vec()).expect("encode unwrap ids query"),
        )
        .unwrap_or_else(|err| panic!("gateway unwrap ids query failed: {err}"));
    Decode!(&out, Vec<Vec<u8>>).expect("decode unwrap ids")
}

fn wrap_request_status(
    pic: &PocketIc,
    wrap_id: Principal,
    request_id: &[u8],
) -> Option<WrapRequestStatus> {
    let out = pic
        .query_call(
            wrap_id,
            Principal::anonymous(),
            "get_request",
            Encode!(&request_id.to_vec()).expect("encode wrap status query"),
        )
        .unwrap_or_else(|err| panic!("wrap status query failed: {err}"));
    Decode!(&out, Option<RequestOverview>)
        .expect("decode wrap status")
        .map(|value| value.status)
}

#[test]
fn upgrade_retries_dispatching_unwrap_via_idempotent_submit() {
    let pic = PocketIc::new();
    let (gateway_id, wrap_id) = install_pair(&pic);
    let asset = Principal::self_authenticating(b"unwrap-recovery-e2e-asset");
    let recipient = Principal::self_authenticating(b"unwrap-recovery-e2e-recipient");
    setup_unwrap_state(&pic, gateway_id, asset);
    let payload = encode_unwrap_payload(asset, recipient);

    let tx_id = submit_unwrap_tx(&pic, gateway_id, payload);

    pic.advance_time(Duration::from_secs(60));
    pic.tick();
    let request_ids = gateway_unwrap_request_ids_by_tx_id(&pic, gateway_id, &tx_id);
    assert_eq!(request_ids.len(), 1);
    let request_id = request_ids[0].clone();
    assert_eq!(
        gateway_dispatch_result(&pic, gateway_id, request_id.as_slice()).map(|value| value.status),
        Some(RequestDispatchStatusView::Queued)
    );
    seed_wrap_request(&pic, wrap_id, request_id.as_slice(), asset, recipient);
    assert_eq!(
        wrap_request_status(&pic, wrap_id, request_id.as_slice()),
        Some(WrapRequestStatus::Queued)
    );

    pic.upgrade_canister(
        gateway_id,
        read_wasm(gateway_wasm_path()),
        Encode!(&Some(GatewayInitArgs {
            genesis_balances: vec![GenesisBalanceView {
                address: hash::derive_evm_address_from_principal(test_caller().as_slice())
                    .expect("derive caller evm address")
                    .to_vec(),
                amount: 1_000_000_000_000_000_000u128,
            }],
            wrap_canister_id: wrap_id,
            wrap_factory_address: predict_create_address(
                hash::derive_evm_address_from_principal(test_caller().as_slice())
                    .expect("derive caller evm address"),
                0,
            )
            .to_vec(),
        }))
        .expect("encode upgrade arg"),
        Some(test_caller()),
    )
    .unwrap_or_else(|err| panic!("upgrade failed: {err}"));

    let mut final_result = None;
    let mut last_result = None;
    for _ in 0..12 {
        pic.advance_time(Duration::from_secs(1));
        pic.tick();
        let result = gateway_dispatch_result(&pic, gateway_id, &request_id);
        last_result = result.clone();
        if result.as_ref().map(|value| &value.status)
            == Some(&RequestDispatchStatusView::Dispatched)
        {
            final_result = result;
            break;
        }
    }

    let result = final_result.unwrap_or_else(|| {
        panic!("gateway did not recover idempotent unwrap after upgrade: {last_result:?}")
    });
    assert_eq!(result.status, RequestDispatchStatusView::Dispatched);
    assert_eq!(result.error, None);
    assert!(wrap_request_status(&pic, wrap_id, request_id.as_slice()).is_some());
}
