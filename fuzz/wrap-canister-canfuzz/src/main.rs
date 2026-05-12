//! where: standalone wrap-canister coverage fuzzer
//! what: run canfuzz against dispatch_unwrap_request and verify that successful inserts remain queryable
//! why: this canister has input validation and idempotency logic that benefits from stateful coverage-guided exploration

mod input;

use crate::input::DispatchInput;
use candid::{decode_one, encode_one, CandidType, Deserialize, Principal};
use canfuzz::fuzzer::{CanisterBuilder, FuzzerBuilder};
use canfuzz::instrumentation::{instrument_wasm_for_fuzzing, InstrumentationArgs, Seed};
use canfuzz::libafl::executors::ExitKind;
use canfuzz::libafl::inputs::BytesInput;
use canfuzz::orchestrator::FuzzerOrchestrator;
use std::fs;
use std::path::{Path, PathBuf};

const FUZZER_NAME: &str = "wrap_canister_dispatch_unwrap";
const METHOD_DISPATCH: &str = "dispatch_unwrap_request";
const METHOD_GET_REQUEST: &str = "get_request";
const ENV_WASM_PATH: &str = "WRAP_CANISTER_WASM";
const ENV_REPLAY_PATH: &str = "WRAP_CANISTER_FUZZ_ONE_INPUT";
const INSTRUMENTED_WASM_NAME: &str = "wrap_canister.canfuzz.wasm";

canfuzz::define_fuzzer_state!(WrapCanisterFuzzer);

#[derive(Clone, Debug, CandidType, Deserialize)]
struct DispatchUnwrapRequestOk {
    request_id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum ApiError {
    InvalidArgument(ApiErrorDetail),
    Internal(ApiErrorDetail),
    Rejected(ApiErrorDetail),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ApiErrorDetail {
    code: String,
    message: String,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct RequestOverview {
    request_id: Vec<u8>,
}

impl FuzzerOrchestrator for WrapCanisterFuzzer {
    fn init(&mut self) {
        self.as_mut().setup_canisters();
    }

    fn corpus_dir(&self) -> PathBuf {
        PathBuf::from("fuzz/wrap-canister-canfuzz/corpus")
    }

    fn execute(&self, input: BytesInput) -> ExitKind {
        let dispatch = DispatchInput::from_bytes(&Vec::<u8>::from(input));
        let arg = encode_one(dispatch.to_args()).expect("dispatch candid encode");
        let pic = self.get_state_machine();
        let canister_id = self.get_coverage_canister_id();

        let result = match pic.update_call(canister_id, kasane_principal(), METHOD_DISPATCH, arg) {
            Ok(result) => result,
            Err(_) => return ExitKind::Crash,
        };

        let decoded = decode_one::<Result<DispatchUnwrapRequestOk, ApiError>>(&result)
            .expect("dispatch result candid decode");
        let ok = match decoded {
            Ok(ok) => ok,
            Err(ApiError::InvalidArgument(_))
            | Err(ApiError::Internal(_))
            | Err(ApiError::Rejected(_)) => return ExitKind::Ok,
        };

        let query_arg = encode_one(ok.request_id.clone()).expect("query candid encode");
        let out = match pic.query_call(
            canister_id,
            Principal::anonymous(),
            METHOD_GET_REQUEST,
            query_arg,
        ) {
            Ok(out) => out,
            Err(_) => return ExitKind::Crash,
        };

        let overview = decode_one::<Option<RequestOverview>>(&out).expect("query candid decode");
        if overview.as_ref().map(|entry| entry.request_id.as_slice()) == Some(ok.request_id.as_slice())
        {
            ExitKind::Ok
        } else {
            ExitKind::Crash
        }
    }
}

fn main() {
    let instrumented_wasm = prepare_instrumented_wasm();
    let wrap_canister = CanisterBuilder::new("wrap_canister")
        .with_wasm_path(instrumented_wasm)
        .with_init_args(Some(encode_one(init_args()).expect("init candid encode")))
        .as_coverage()
        .build();
    let state = FuzzerBuilder::new()
        .name(FUZZER_NAME)
        .with_canister(wrap_canister)
        .build();
    let mut fuzzer = WrapCanisterFuzzer(state);

    if let Some(path) = replay_input_path() {
        let input = fs::read(path).expect("read crash input");
        fuzzer.test_one_input(input);
        return;
    }

    fuzzer.run();
}

fn wrap_canister_wasm_path() -> String {
    std::env::var(ENV_WASM_PATH).unwrap_or_else(|_| {
        repo_root()
            .join("target/wasm32-unknown-unknown/release/wrap_canister.wasm")
            .display()
            .to_string()
    })
}

fn prepare_instrumented_wasm() -> String {
    let wasm_path = wrap_canister_wasm_path();
    let wasm_bytes = fs::read(&wasm_path).unwrap_or_else(|err| {
        panic!("failed to read wrap-canister wasm from {wasm_path}: {err}")
    });
    let instrumented = instrument_wasm_for_fuzzing(InstrumentationArgs {
        wasm_bytes,
        history_size: 1,
        seed: Seed::Static(0xCAFE_BABE),
        instrument_instruction_count: false,
    });
    let out_path = out_dir().join(INSTRUMENTED_WASM_NAME);
    fs::create_dir_all(out_path.parent().expect("instrumented wasm parent"))
        .expect("create canfuzz out dir");
    fs::write(&out_path, instrumented).expect("write instrumented wasm");
    out_path.display().to_string()
}

fn replay_input_path() -> Option<PathBuf> {
    std::env::var_os(ENV_REPLAY_PATH).map(PathBuf::from)
}

fn out_dir() -> PathBuf {
    std::env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root().join("target/canfuzz-out"))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root path")
}

fn kasane_principal() -> Principal {
    Principal::self_authenticating(b"wrap-canister-fuzz-kasane")
}

fn init_args() -> InitArgs {
    InitArgs {
        kasane_canister: kasane_principal(),
        evm_gateway_canister: Principal::self_authenticating(b"wrap-canister-fuzz-gateway"),
        fee_ledger_canister: Principal::self_authenticating(b"wrap-canister-fuzz-ledger"),
        native_ledger_canister: Principal::self_authenticating(b"wrap-canister-fuzz-native"),
        wrap_factory_address: vec![0x11; 20],
        cycle_fee_e8s: 1_000_000,
        gas_price_buffer_bps: 12_000,
        allowed_assets: vec![Principal::self_authenticating(b"wrap-canister-fuzz-asset")],
    }
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct InitArgs {
    kasane_canister: Principal,
    evm_gateway_canister: Principal,
    fee_ledger_canister: Principal,
    native_ledger_canister: Principal,
    wrap_factory_address: Vec<u8>,
    cycle_fee_e8s: u64,
    gas_price_buffer_bps: u32,
    allowed_assets: Vec<Principal>,
}
