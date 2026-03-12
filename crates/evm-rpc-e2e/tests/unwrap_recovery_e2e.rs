//! どこで: unwrap dispatch の PocketIC E2E
//! 何を: upgrade 後の unwrap 再送が idempotent に復旧することを確認
//! なぜ: gateway の Dispatching 再開と wrap 側 idempotency の回帰を防ぐため

use candid::{CandidType, Decode, Deserialize, Encode, Nat, Principal};
use evm_core::wrap_precompile::WRAP_PRECOMPILE_ADDRESS;
use pocket_ic::PocketIc;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct GenesisBalanceView {
    address: Vec<u8>,
    amount: u128,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct GatewayInitArgs {
    genesis_balances: Vec<GenesisBalanceView>,
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

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum SubmitTxError {
    InvalidArgument(String),
    Rejected(String),
    Internal(String),
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

    let gateway_init = Some(GatewayInitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: hash::derive_evm_address_from_principal(test_caller().as_slice())
                .expect("derive caller evm address")
                .to_vec(),
            amount: 1_000_000_000_000_000_000u128,
        }],
    });
    pic.install_canister(wrap_id, read_wasm(wrap_wasm_path()), Encode!(&()).expect("encode mock wrap init"), None);
    pic.install_canister(
        gateway_id,
        read_wasm(gateway_wasm_path()),
        Encode!(&gateway_init).expect("encode gateway init"),
        None,
    );
    pic.set_controllers(gateway_id, Some(Principal::anonymous()), vec![test_caller()])
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

fn submit_unwrap_tx(pic: &PocketIc, gateway_id: Principal, data: Vec<u8>) -> Vec<u8> {
    let out = pic
        .update_call(
            gateway_id,
            test_caller(),
            "submit_ic_tx",
            Encode!(&SubmitIcTxArgsDto {
                to: Some(WRAP_PRECOMPILE_ADDRESS.into_array().to_vec()),
                from: None,
                value: candid::Nat::from(0u8),
                max_priority_fee_per_gas: candid::Nat::from(300_000_000_000u64),
                data,
                max_fee_per_gas: candid::Nat::from(600_000_000_000u64),
                nonce: 0,
                gas_limit: 300_000,
            })
            .expect("encode submit"),
        )
        .unwrap_or_else(|err| panic!("submit update failed: {err}"));
    let result: Result<Vec<u8>, SubmitTxError> = Decode!(&out, Result<Vec<u8>, SubmitTxError>)
        .expect("decode submit result");
    result.unwrap_or_else(|err| panic!("submit failed: {err:?}"))
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

fn gateway_dispatch_result(pic: &PocketIc, gateway_id: Principal, request_id: &[u8]) -> Option<UnwrapDispatchOverviewView> {
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

fn wrap_request_status(pic: &PocketIc, wrap_id: Principal, request_id: &[u8]) -> Option<WrapRequestStatus> {
    let out = pic
        .query_call(
            wrap_id,
            Principal::anonymous(),
            "get_request",
            Encode!(&request_id.to_vec()).expect("encode wrap status query"),
        )
        .unwrap_or_else(|err| panic!("wrap status query failed: {err}"));
    Decode!(&out, Option<RequestOverview>).expect("decode wrap status")
        .map(|value| value.status)
}

#[test]
fn upgrade_retries_dispatching_unwrap_via_idempotent_submit() {
    let pic = PocketIc::new();
    let (gateway_id, wrap_id) = install_pair(&pic);
    let asset = Principal::self_authenticating(b"unwrap-recovery-e2e-asset");
    let recipient = Principal::self_authenticating(b"unwrap-recovery-e2e-recipient");
    let payload = encode_unwrap_payload(asset, recipient);

    let tx_id = submit_unwrap_tx(&pic, gateway_id, payload);

    pic.advance_time(Duration::from_secs(60));
    pic.tick();
    let request_ids = gateway_unwrap_request_ids_by_tx_id(&pic, gateway_id, &tx_id);
    assert_eq!(request_ids.len(), 1);
    let request_id = request_ids[0].clone();
    assert_eq!(
        gateway_dispatch_result(&pic, gateway_id, request_id.as_slice())
            .map(|value| value.status),
        Some(RequestDispatchStatusView::Queued)
    );
    seed_wrap_request(&pic, wrap_id, request_id.as_slice(), asset, recipient);
    assert_eq!(wrap_request_status(&pic, wrap_id, request_id.as_slice()), Some(WrapRequestStatus::Queued));

    pic.upgrade_canister(
        gateway_id,
        read_wasm(gateway_wasm_path()),
        Encode!(&()).expect("encode empty upgrade arg"),
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
        if result.as_ref().map(|value| &value.status) == Some(&RequestDispatchStatusView::Dispatched) {
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
