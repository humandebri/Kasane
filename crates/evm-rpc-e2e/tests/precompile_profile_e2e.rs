//! どこで: PocketIC E2E / 何を: precompile profile を実測する専用導線 / なぜ: ratio 調整を IC 命令数ベースで再現可能にするため

use candid::{CandidType, Decode, Encode, Principal};
use evm_core::hash;
use pocket_ic::PocketIc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug, CandidType, Deserialize)]
struct GenesisBalanceView {
    address: Vec<u8>,
    amount: u128,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct InitArgs {
    genesis_balances: Vec<GenesisBalanceView>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct RpcCallObjectView {
    to: Option<Vec<u8>>,
    from: Option<Vec<u8>>,
    gas: Option<u64>,
    gas_price: Option<u128>,
    nonce: Option<u64>,
    max_fee_per_gas: Option<u128>,
    max_priority_fee_per_gas: Option<u128>,
    chain_id: Option<u64>,
    tx_type: Option<u64>,
    access_list: Option<Vec<RpcAccessListItemView>>,
    value: Option<Vec<u8>>,
    data: Option<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct RpcAccessListItemView {
    address: Vec<u8>,
    storage_keys: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct RpcCallResultView {
    status: u8,
    gas_used: u64,
    return_data: Vec<u8>,
    revert_data: Option<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct PrecompileProfileView {
    address: Vec<u8>,
    calls: u64,
    total_instructions: u128,
    avg_instructions: u64,
    max_instructions: u64,
    total_extra_gas: u128,
    avg_extra_gas: u64,
    max_extra_gas: u64,
}

#[derive(Clone, Debug, Serialize)]
struct ProfileReport {
    runs: usize,
    targets: Vec<String>,
    entries: Vec<ProfileEntryReport>,
}

#[derive(Clone, Debug, Serialize)]
struct ProfileEntryReport {
    name: String,
    address: String,
    calls: u64,
    avg_instructions: u64,
    max_instructions: u64,
    avg_extra_gas: u64,
    max_extra_gas: u64,
}

fn wasm_path() -> PathBuf {
    if let Some(path) = std::env::var_os("IC_EVM_GATEWAY_WASM") {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("ic_evm_gateway.wasm")
}

fn test_caller() -> Principal {
    Principal::self_authenticating(b"precompile-profile-e2e")
}

fn install_canister(pic: &PocketIc) -> Principal {
    let caller = test_caller();
    let init = Some(InitArgs {
        genesis_balances: vec![GenesisBalanceView {
            address: hash::derive_evm_address_from_principal(caller.as_slice())
                .expect("must derive caller address")
                .to_vec(),
            amount: 10_000_000_000_000_000_000_000_000u128,
        }],
    });
    let wasm = fs::read(wasm_path()).expect("read gateway wasm");
    let canister_id = pic.create_canister();
    pic.add_cycles(canister_id, 5_000_000_000_000u128);
    pic.install_canister(
        canister_id,
        wasm,
        Encode!(&init).expect("encode init args"),
        None,
    );
    pic.set_controllers(canister_id, Some(Principal::anonymous()), vec![caller])
        .unwrap_or_else(|err| panic!("set_controllers error: {err}"));
    settle_migrations(pic);
    canister_id
}

fn settle_migrations(pic: &PocketIc) {
    for _ in 0..6 {
        pic.advance_time(Duration::from_secs(60));
        pic.tick();
    }
}

fn call_update(pic: &PocketIc, canister_id: Principal, method: &str, arg: Vec<u8>) -> Vec<u8> {
    pic.update_call(canister_id, test_caller(), method, arg)
        .unwrap_or_else(|err| panic!("update error on {method}: {err}"))
}

fn call_query_as(
    pic: &PocketIc,
    canister_id: Principal,
    caller: Principal,
    method: &str,
    arg: Vec<u8>,
) -> Vec<u8> {
    pic.query_call(canister_id, caller, method, arg)
        .unwrap_or_else(|err| panic!("query error on {method}: {err}"))
}

fn address_from_u64(value: u64) -> Vec<u8> {
    let mut out = vec![0u8; 20];
    out[12..].copy_from_slice(&value.to_be_bytes());
    out
}

fn decode_hex(input: &str) -> Vec<u8> {
    fn nibble(byte: u8) -> u8 {
        match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => panic!("invalid hex input"),
        }
    }
    let bytes = input.as_bytes();
    assert_eq!(bytes.len() % 2, 0, "hex length must be even");
    bytes
        .chunks_exact(2)
        .map(|pair| (nibble(pair[0]) << 4) | nibble(pair[1]))
        .collect()
}

fn ecrecover_input() -> Vec<u8> {
    let mut out = vec![0u8; 128];
    for (idx, byte) in out[..32].iter_mut().enumerate() {
        *byte = u8::try_from(idx + 1).expect("msg byte");
    }
    out[63] = 27;
    for (idx, byte) in out[64..128].iter_mut().enumerate() {
        *byte = u8::try_from(idx + 3).expect("sig byte");
    }
    out
}

fn blake2f_input(rounds: u32) -> Vec<u8> {
    let mut out = vec![0u8; 213];
    out[..4].copy_from_slice(&rounds.to_be_bytes());
    out[212] = 1;
    out
}

fn modexp_input() -> Vec<u8> {
    decode_hex(
        "0000000000000000000000000000000000000000000000000000000000000001\
         0000000000000000000000000000000000000000000000000000000000000001\
         0000000000000000000000000000000000000000000000000000000000000001\
         05\
         07\
         0d",
    )
}

fn modexp_heavy_input() -> Vec<u8> {
    decode_hex(
        "0000000000000000000000000000000000000000000000000000000000000020\
         0000000000000000000000000000000000000000000000000000000000000020\
         0000000000000000000000000000000000000000000000000000000000000020\
         ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\
         ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\
         fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd",
    )
}

fn p256_input() -> Vec<u8> {
    decode_hex("b5a77e7a90aa14e0bf5f337f06f597148676424fae26e175c6e5621c34351955289f319789da424845c9eac935245fcddd805950e2f02506d09be7e411199556d262144475b1fa46ad85250728c600c53dfd10f8b3f4adf140e27241aec3c2da3a81046703fccf468b48b145f939efdbb96c3786db712b3113bb2488ef286cdcef8afe82d200a5bb36b5462166e8ce77f2d831a52ef2135b2af188110beaefb1")
}

fn workload_specs(selected: &[String]) -> Vec<(String, u64, u64, Vec<u8>)> {
    let mut specs = Vec::new();
    for target in selected {
        match target.as_str() {
            "ecrecover" => specs.push((target.clone(), 1, 500_000, ecrecover_input())),
            "modexp" => specs.push((target.clone(), 5, 500_000, modexp_input())),
            "modexp_heavy" => specs.push((target.clone(), 5, 5_500_000, modexp_heavy_input())),
            // fixed ratio 1/100 adds about 553k gas to the 100k-round fixture on PocketIC.
            "blake2f" => specs.push((target.clone(), 9, 1_500_000, blake2f_input(100_000))),
            "p256" => specs.push((target.clone(), 256, 500_000, p256_input())),
            other => panic!("unsupported precompile target: {other}"),
        }
    }
    specs
}

fn selected_targets() -> Vec<String> {
    let raw = std::env::var("PRECOMPILE_PROFILE_TARGETS")
        .unwrap_or_else(|_| "ecrecover,blake2f,modexp".to_string());
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn configured_runs() -> usize {
    std::env::var("PRECOMPILE_PROFILE_RUNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(30)
}

fn precompile_call(address_u64: u64, gas_limit: u64, from: [u8; 20], data: Vec<u8>) -> RpcCallObjectView {
    RpcCallObjectView {
        to: Some(address_from_u64(address_u64)),
        from: Some(from.to_vec()),
        gas: Some(gas_limit),
        gas_price: Some(600_000_000_000),
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: Some(vec![0u8; 32]),
        data: Some(data),
    }
}

#[test]
fn precompile_profile_is_measurable_with_pocket_ic() {
    let targets = selected_targets();
    let runs = configured_runs();
    let specs = workload_specs(&targets);
    assert!(!specs.is_empty(), "at least one target is required");
    let pic = PocketIc::new();
    let canister_id = install_canister(&pic);

    let caller_address = hash::derive_evm_address_from_principal(test_caller().as_slice())
        .expect("derive caller");
    for _ in 0..runs {
        for (name, address_u64, gas_limit, data) in &specs {
            let call_out = call_update(
                &pic,
                canister_id,
                "profile_precompile_call",
                Encode!(&precompile_call(*address_u64, *gas_limit, caller_address, data.clone()))
                    .expect("encode precompile call"),
            );
            let call_result: Result<RpcCallResultView, String> =
                Decode!(&call_out, Result<RpcCallResultView, String>).expect("decode call");
            let call_result = call_result.unwrap_or_else(|err| panic!("call failed for {name}: {err}"));
            assert_eq!(call_result.status, 1, "call must succeed for {name}");
        }
    }

    let profile_out = call_query_as(
        &pic,
        canister_id,
        test_caller(),
        "get_precompile_profile",
        Encode!(&()).expect("encode profile query"),
    );
    let profile: Vec<PrecompileProfileView> =
        Decode!(&profile_out, Vec<PrecompileProfileView>).expect("decode profile");

    let mut entries = Vec::new();
    for (name, address_u64, _, _) in &specs {
        let address = address_from_u64(*address_u64);
        let entry = profile
            .iter()
            .find(|item| item.address == address)
            .unwrap_or_else(|| panic!("missing profile entry for {name}"));
        assert_eq!(entry.calls, runs as u64, "unexpected calls for {name}");
        assert!(entry.avg_instructions > 0, "avg_instructions must be > 0 for {name}");
        assert!(entry.avg_extra_gas > 0, "avg_extra_gas must be > 0 for {name}");
        entries.push(ProfileEntryReport {
            name: name.clone(),
            address: format!("0x{}", address.iter().map(|byte| format!("{byte:02x}")).collect::<String>()),
            calls: entry.calls,
            avg_instructions: entry.avg_instructions,
            max_instructions: entry.max_instructions,
            avg_extra_gas: entry.avg_extra_gas,
            max_extra_gas: entry.max_extra_gas,
        });
    }

    let report = ProfileReport { runs, targets, entries };
    let report_json = serde_json::to_string_pretty(&report).expect("serialize report");
    println!("{report_json}");
    if let Ok(path) = std::env::var("PRECOMPILE_PROFILE_JSON_PATH") {
        fs::write(path, report_json).expect("write profile report");
    }
}

#[test]
fn get_precompile_profile_rejects_non_controller_query() {
    let pic = PocketIc::new();
    let canister_id = install_canister(&pic);
    let non_controller = Principal::self_authenticating(b"precompile-profile-non-controller");
    let err = pic
        .query_call(
            canister_id,
            non_controller,
            "get_precompile_profile",
            Encode!(&()).expect("encode profile query"),
        )
        .expect_err("non-controller query must be rejected");
    assert!(
        err.to_string().contains("auth.controller_required"),
        "unexpected query error: {err}"
    );
}
