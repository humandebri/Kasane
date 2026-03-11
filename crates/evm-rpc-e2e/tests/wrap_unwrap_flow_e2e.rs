//! どこで: wrap / unwrap の PocketIC E2E
//! 何を: real wrap canister と gateway 間の経路を運用に近い形で固定
//! なぜ: gas price の canister 間呼び出し種別と unwrap dispatch の実挙動を回帰から守るため

use candid::{CandidType, Decode, Deserialize, Encode, Nat, Principal};
use evm_core::hash;
use evm_core::wrap_precompile::WRAP_PRECOMPILE_ADDRESS;
use pocket_ic::PocketIc;
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
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct WrapInitArgs {
    kasane_canister: Principal,
    evm_gateway_canister: Principal,
    fee_ledger_canister: Principal,
    cycle_fee_e8s: u64,
    gas_price_buffer_bps: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct SubmitIcTxArgsDto {
    to: Option<Vec<u8>>,
    value: Nat,
    max_priority_fee_per_gas: Nat,
    data: Vec<u8>,
    max_fee_per_gas: Nat,
    nonce: u64,
    gas_limit: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct SubmitWrapRequestArgs {
    request_id: Vec<u8>,
    asset_id: Vec<u8>,
    amount: Vec<u8>,
    evm_recipient: Vec<u8>,
    evm_nonce: u64,
    gas_limit: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct SubmitWrapRequestOk {
    request_id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum SubmitTxError {
    InvalidArgument(String),
    Rejected(String),
    Internal(String),
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum RequestKindView {
    Unwrap,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum RequestDispatchStatusView {
    Queued,
    Dispatching,
    Dispatched,
    DispatchFailed,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct RequestDispatchResultView {
    status: RequestDispatchStatusView,
    error_code: Option<String>,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum WrapRequestStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

fn gateway_wasm_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/wasm32-unknown-unknown/release/ic_evm_gateway.wasm")
}

fn wrap_wasm_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/wasm32-unknown-unknown/release/wrap_canister.wasm")
}

fn read_wasm(path: PathBuf) -> Vec<u8> {
    if !path.exists() {
        panic!("wasm not found: build release wasm first: {path:?}");
    }
    std::fs::read(path).expect("read wasm")
}

fn test_caller() -> Principal {
    Principal::self_authenticating(b"wrap-unwrap-e2e-caller")
}

fn install_pair(pic: &PocketIc) -> (Principal, Principal, Principal) {
    let gateway_id = pic.create_canister();
    let wrap_id = pic.create_canister();
    let fee_ledger_id = pic.create_canister();
    let kasane_id = pic.create_canister();
    for canister_id in [gateway_id, wrap_id, fee_ledger_id, kasane_id] {
        pic.add_cycles(canister_id, 5_000_000_000_000u128);
    }

    let caller = test_caller();
    let gateway_init = Some(GatewayInitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: hash::derive_evm_address_from_principal(caller.as_slice())
                .expect("derive caller evm address")
                .to_vec(),
            amount: 1_000_000_000_000_000_000u128,
        }],
    });
    let wrap_init = WrapInitArgs {
        kasane_canister: gateway_id,
        evm_gateway_canister: gateway_id,
        fee_ledger_canister: fee_ledger_id,
        cycle_fee_e8s: 1_000_000,
        gas_price_buffer_bps: 12_000,
    };

    pic.install_canister(wrap_id, read_wasm(wrap_wasm_path()), Encode!(&wrap_init).expect("encode wrap init"), None);
    pic.install_canister(
        gateway_id,
        read_wasm(gateway_wasm_path()),
        Encode!(&gateway_init).expect("encode gateway init"),
        None,
    );
    settle(pic, 6);
    (gateway_id, wrap_id, fee_ledger_id)
}

fn settle(pic: &PocketIc, rounds: usize) {
    for _ in 0..rounds {
        pic.advance_time(Duration::from_secs(60));
        pic.tick();
    }
}

fn build_submit_ic_tx_args(to: [u8; 20], nonce: u64, data: Vec<u8>, gas_limit: u64) -> SubmitIcTxArgsDto {
    SubmitIcTxArgsDto {
        to: Some(to.to_vec()),
        value: Nat::from(0u8),
        max_priority_fee_per_gas: Nat::from(300_000_000_000u64),
        data,
        max_fee_per_gas: Nat::from(600_000_000_000u64),
        nonce,
        gas_limit,
    }
}

fn submit_ic_tx(pic: &PocketIc, gateway_id: Principal, args: SubmitIcTxArgsDto) -> Vec<u8> {
    for _ in 0..4 {
        let out = pic.update_call(gateway_id, test_caller(), "submit_ic_tx", Encode!(&args).expect("encode submit")).unwrap();
        let result: Result<Vec<u8>, SubmitTxError> = Decode!(&out, Result<Vec<u8>, SubmitTxError>).expect("decode submit");
        match result {
            Ok(tx_id) => return tx_id,
            Err(SubmitTxError::Rejected(message)) if message == "ops.write.needs_migration" => settle(pic, 1),
            Err(err) => panic!("submit failed: {err:?}"),
        }
    }
    panic!("submit did not succeed after migration retries");
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

fn wrap_request_id(principal: Principal, asset_id: &[u8], amount: &[u8], evm_recipient: &[u8], evm_nonce: u64, gas_limit: u64) -> Vec<u8> {
    fn hash_len_prefixed(hasher: &mut Keccak, bytes: &[u8]) { hasher.update(&(bytes.len() as u32).to_be_bytes()); hasher.update(bytes); }
    let mut keccak = Keccak::v256();
    keccak.update(b"kasane.wrap.request.v1");
    hash_len_prefixed(&mut keccak, principal.as_slice());
    hash_len_prefixed(&mut keccak, asset_id);
    hash_len_prefixed(&mut keccak, amount);
    hash_len_prefixed(&mut keccak, evm_recipient);
    keccak.update(&evm_nonce.to_be_bytes());
    keccak.update(&gas_limit.to_be_bytes());
    let mut out = [0u8; 32];
    keccak.finalize(&mut out);
    out.to_vec()
}

#[test]
fn wrap_submit_request_reaches_fee_collection_after_gateway_gas_quote() {
    let pic = PocketIc::new();
    let (gateway_id, wrap_id, fee_ledger_id) = install_pair(&pic);
    submit_ic_tx(&pic, gateway_id, build_submit_ic_tx_args([0x10; 20], 0, Vec::new(), 50_000));
    settle(&pic, 3);

    let asset_id = fee_ledger_id.as_slice().to_vec();
    let amount = {
        let mut out = [0u8; 32];
        out[16..].copy_from_slice(&1_000_000u128.to_be_bytes());
        out.to_vec()
    };
    let evm_recipient = vec![0x55; 20];
    let request_id = wrap_request_id(test_caller(), &asset_id, &amount, &evm_recipient, 7, 150_000);
    let args = SubmitWrapRequestArgs { request_id, asset_id, amount, evm_recipient, evm_nonce: 7, gas_limit: 150_000 };
    let out = pic.update_call(wrap_id, test_caller(), "submit_wrap_request", Encode!(&args).expect("encode wrap submit")).unwrap();
    let result: Result<SubmitWrapRequestOk, String> = Decode!(&out, Result<SubmitWrapRequestOk, String>).expect("decode wrap submit");
    let err = result.expect_err("dummy fee ledger should fail");
    assert!(err.starts_with("fee.call_failed:"), "unexpected wrap submit error: {err}");
    assert!(!err.contains("fee.quote_"), "gas quote path should already be fixed: {err}");
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
        .unwrap();
    Decode!(&out, Vec<Vec<u8>>).expect("decode unwrap ids")
}

#[test]
fn unwrap_dispatch_succeeds_with_real_wrap_canister() {
    let pic = PocketIc::new();
    let (gateway_id, wrap_id, _) = install_pair(&pic);
    let asset = Principal::self_authenticating(b"wrap-unwrap-e2e-asset");
    let recipient = Principal::self_authenticating(b"wrap-unwrap-e2e-recipient");
    let data = encode_unwrap_payload(asset, recipient);

    let tx_id = submit_ic_tx(
        &pic,
        gateway_id,
        build_submit_ic_tx_args(WRAP_PRECOMPILE_ADDRESS.into_array(), 0, data, 300_000),
    );

    let mut final_result = None;
    let mut last_result = None;
    let mut request_id = None;
    for _ in 0..12 {
        settle(&pic, 1);
        if request_id.is_none() {
            let request_ids = gateway_unwrap_request_ids_by_tx_id(&pic, gateway_id, &tx_id);
            if request_ids.len() == 1 {
                request_id = request_ids.into_iter().next();
            }
        }
        let Some(ref request_id) = request_id else {
            continue;
        };
        let out = pic.query_call(gateway_id, Principal::anonymous(), "get_request_dispatch_result", Encode!(&RequestKindView::Unwrap, request_id).unwrap()).unwrap();
        let result: Option<RequestDispatchResultView> = Decode!(&out, Option<RequestDispatchResultView>).expect("decode dispatch result");
        last_result = result.clone();
        if result.as_ref().map(|value| &value.status) == Some(&RequestDispatchStatusView::Dispatched) {
            final_result = result;
            break;
        }
    }

    let result = final_result.unwrap_or_else(|| panic!("unwrap should dispatch, last_result={last_result:?}"));
    assert_eq!(result.status, RequestDispatchStatusView::Dispatched);
    assert_eq!(result.error_code, None);
    let request_id = request_id.expect("unwrap request id must resolve from tx");
    let wrap_status_out = pic.query_call(wrap_id, Principal::anonymous(), "get_request_status", Encode!(&request_id).unwrap()).unwrap();
    let wrap_status: Option<WrapRequestStatus> = Decode!(&wrap_status_out, Option<WrapRequestStatus>).expect("decode wrap request status");
    assert_eq!(wrap_status, Some(WrapRequestStatus::Queued));
}
