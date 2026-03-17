//! どこで: ic-evm-rpc の query microbench
//! 何を: 代表的な eth_call 系 query のネイティブ実行時間を計測
//! なぜ: canbench が取りづらい環境でも相対比較できる材料を残すため

use alloy_consensus::{SignableTransaction, TxEip1559};
use alloy_eips::eip2718::Encodable2718;
use alloy_eips::eip2930::AccessList;
use alloy_primitives::{Address, Bytes, TxKind as EthTxKind, U256 as AlloyU256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use evm_core::chain;
use evm_core::hash;
use evm_core::wrap_precompile::WRAP_PRECOMPILE_ADDRESS;
use evm_db::chain_data::constants::CHAIN_ID;
use evm_db::stable_state::{init_stable_state, set_runtime_config, with_state_mut};
use evm_db::types::keys::{make_account_key, make_storage_key};
use evm_db::types::values::{AccountVal, U256Val};
use evm_db::Storable;
use ic_evm_rpc::{rpc_eth_call_object, rpc_eth_call_rawtx, rpc_eth_estimate_gas_object};
use ic_evm_rpc_types::RpcCallObjectView;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const ITERS: usize = 200;
const BASIC_FROM: [u8; 20] = [0x77u8; 20];
const WRAP_QUERY_FACTORY_ADDRESS: [u8; 20] = [0x55u8; 20];
const WRAPPED_TOKEN_ADDRESS: [u8; 20] = [0x42u8; 20];
const WRAP_QUERY_AMOUNT: u64 = 1_000_000_000_000;

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
#[ignore = "manual microbench; run with --ignored --nocapture"]
fn microbench_rpc_eth_queries() {
    let _guard = test_lock().lock().expect("lock");

    init_stable_state();
    seed_basic_call_fixture();
    seed_rawtx_fixture();
    seed_wrap_query_fixture();

    let basic_call = basic_call_object();
    let wrap_call = wrap_precompile_call_object();
    let raw_tx = build_eth_signed_1559(0, 2_000_000_000u128, 1_000_000_000u128);

    print_bench("rpc_eth_call_object/basic", || {
        let _ = rpc_eth_call_object(basic_call.clone()).expect("basic eth_call");
    });
    print_bench("rpc_eth_estimate_gas_object/basic", || {
        let _ = rpc_eth_estimate_gas_object(basic_call.clone()).expect("basic estimate");
    });
    print_bench("rpc_eth_call_rawtx/legacy_1559", || {
        let _ = rpc_eth_call_rawtx(raw_tx.clone()).expect("raw eth_call");
    });
    print_bench("rpc_eth_call_object/wrap_precompile", || {
        let _ = rpc_eth_call_object(wrap_call.clone()).expect("wrap eth_call");
    });
    print_bench("rpc_eth_estimate_gas_object/wrap_precompile", || {
        let _ = rpc_eth_estimate_gas_object(wrap_call.clone()).expect("wrap estimate");
    });
}

fn print_bench(label: &str, mut f: impl FnMut()) {
    let mut samples = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let start = Instant::now();
        f();
        samples.push(start.elapsed());
    }
    samples.sort_unstable();
    let total = samples
        .iter()
        .copied()
        .fold(Duration::ZERO, |acc, value| acc.saturating_add(value));
    let iterations = u32::try_from(ITERS).expect("iters fit into u32");
    let avg = total.div_f64(f64::from(iterations));
    let p50 = percentile(&samples, 50);
    let p95 = percentile(&samples, 95);
    println!(
        "{label}: iters={ITERS} avg_us={} p50_us={} p95_us={}",
        avg.as_micros(),
        p50.as_micros(),
        p95.as_micros()
    );
}

fn percentile(samples: &[Duration], pct: usize) -> Duration {
    let capped = pct.min(100);
    let len = samples.len();
    let rank = ((len.saturating_sub(1)).saturating_mul(capped)) / 100;
    samples[rank]
}

fn seed_basic_call_fixture() {
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1;
        chain_state.min_gas_price = 0;
        chain_state.min_priority_fee = 0;
        state.chain_state.set(chain_state);
        state.accounts.insert(
            make_account_key(BASIC_FROM),
            AccountVal::from_parts(0, [0xffu8; 32], [0u8; 32]),
        );
    });
}

fn seed_rawtx_fixture() {
    let signer = test_signer();
    chain::credit_balance(signer.address().into_array(), 1_000_000_000_000_000_000u128)
        .expect("fund signer");
}

fn basic_call_object() -> RpcCallObjectView {
    RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: Some(BASIC_FROM.to_vec()),
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
    }
}

fn seed_wrap_query_fixture() {
    let caller = [0x31u8; 20];
    let asset = [0x44u8, 0x55, 0x66];
    let amount = u256_from_u64(WRAP_QUERY_AMOUNT);
    let factory_slot = mapping_slot(compute_asset_key(&asset), [0u8; 32]);
    let balance_slot = address_mapping_slot(caller, 3);
    let allowance_slot = allowance_slot(caller, WRAP_QUERY_FACTORY_ADDRESS);

    set_runtime_config(evm_db::chain_data::RuntimeConfigV1::from_bytes(
        std::borrow::Cow::Owned(runtime_config_bytes(WRAP_QUERY_FACTORY_ADDRESS)),
    ));

    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1;
        chain_state.min_gas_price = 1;
        chain_state.min_priority_fee = 1;
        state.chain_state.set(chain_state);

        state.accounts.insert(
            make_account_key(caller),
            AccountVal::from_parts(0, [0xffu8; 32], [0u8; 32]),
        );
        state.accounts.insert(
            make_account_key(WRAP_QUERY_FACTORY_ADDRESS),
            AccountVal::from_parts(1, [0u8; 32], [0x11u8; 32]),
        );
        state.accounts.insert(
            make_account_key(WRAPPED_TOKEN_ADDRESS),
            AccountVal::from_parts(1, [0u8; 32], [0x22u8; 32]),
        );

        let mut token_word = [0u8; 32];
        token_word[12..].copy_from_slice(&WRAPPED_TOKEN_ADDRESS);

        state.storage.insert(
            make_storage_key(WRAP_QUERY_FACTORY_ADDRESS, factory_slot),
            U256Val::new(token_word),
        );
        state.storage.insert(
            make_storage_key(WRAPPED_TOKEN_ADDRESS, u256_from_u64(2)),
            U256Val::new(amount),
        );
        state.storage.insert(
            make_storage_key(WRAPPED_TOKEN_ADDRESS, balance_slot),
            U256Val::new(amount),
        );
        state.storage.insert(
            make_storage_key(WRAPPED_TOKEN_ADDRESS, allowance_slot),
            U256Val::new(amount),
        );
    });
}

fn wrap_precompile_call_object() -> RpcCallObjectView {
    RpcCallObjectView {
        to: Some(WRAP_PRECOMPILE_ADDRESS.as_slice().to_vec()),
        from: Some([0x31u8; 20].to_vec()),
        gas: Some(300_000),
        gas_price: Some(500_000_000_000),
        nonce: Some(0),
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: Some(0),
        access_list: None,
        value: Some(vec![0u8; 32]),
        data: Some(encode_unwrap_input()),
    }
}

fn encode_unwrap_input() -> Vec<u8> {
    let asset = [0x44u8, 0x55, 0x66];
    let recipient = [0x77u8, 0x88, 0x99];
    let mut amount = [0u8; 32];
    amount[16..].copy_from_slice(&u128::from(WRAP_QUERY_AMOUNT).to_be_bytes());

    let mut out = Vec::with_capacity(93);
    out.push(1);
    out.extend_from_slice(&encode_compact_principal(&asset));
    out.extend_from_slice(&amount);
    out.extend_from_slice(&encode_compact_principal(&recipient));
    out
}

fn encode_compact_principal(bytes: &[u8]) -> [u8; 30] {
    let mut out = [0u8; 30];
    out[0] = u8::try_from(bytes.len()).expect("principal len fits in u8");
    out[1..1 + bytes.len()].copy_from_slice(bytes);
    out
}

fn runtime_config_bytes(factory: [u8; 20]) -> Vec<u8> {
    let principal = candid::Principal::self_authenticating(b"wrap-precompile-query");
    let raw = principal.as_slice();
    let mut bytes = [0u8; 64];
    bytes[0] = 1;
    bytes[1] = u8::try_from(raw.len()).expect("principal len fits in u8");
    bytes[2..2 + raw.len()].copy_from_slice(raw);
    bytes[32..52].copy_from_slice(&factory);
    bytes.to_vec()
}

fn compute_asset_key(asset_id: &[u8]) -> [u8; 32] {
    let mut chain_bytes = [0u8; 32];
    chain_bytes[24..].copy_from_slice(&CHAIN_ID.to_be_bytes());
    hash::keccak256(
        &[
            b"kasane.wrap.v1".as_slice(),
            chain_bytes.as_slice(),
            asset_id,
        ]
        .concat(),
    )
}

fn mapping_slot(key: [u8; 32], slot: [u8; 32]) -> [u8; 32] {
    let mut input = [0u8; 64];
    input[..32].copy_from_slice(&key);
    input[32..].copy_from_slice(&slot);
    hash::keccak256(&input)
}

fn address_mapping_slot(key: [u8; 20], slot: u64) -> [u8; 32] {
    let mut key_bytes = [0u8; 32];
    key_bytes[12..].copy_from_slice(&key);
    mapping_slot(key_bytes, u256_from_u64(slot))
}

fn allowance_slot(owner: [u8; 20], spender: [u8; 20]) -> [u8; 32] {
    let outer = address_mapping_slot(owner, 4);
    let mut spender_bytes = [0u8; 32];
    spender_bytes[12..].copy_from_slice(&spender);
    mapping_slot(spender_bytes, outer)
}

fn u256_from_u64(value: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[24..].copy_from_slice(&value.to_be_bytes());
    out
}

fn test_signer() -> PrivateKeySigner {
    "0x59c6995e998f97a5a0044966f094538e0d7f4f4e4d5d8dd6a8c4f9d5f8b1e8a1"
        .parse()
        .expect("signer")
}

fn build_eth_signed_1559(
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
) -> Vec<u8> {
    let signer = test_signer();
    let tx = TxEip1559 {
        chain_id: CHAIN_ID,
        nonce,
        gas_limit: 50_000,
        max_fee_per_gas,
        max_priority_fee_per_gas,
        to: EthTxKind::Call(Address::from([0x21u8; 20])),
        value: AlloyU256::ZERO,
        access_list: AccessList::default(),
        input: Bytes::new(),
    };
    let signature = signer
        .sign_hash_sync(&tx.signature_hash())
        .expect("sign");
    tx.into_signed(signature).encoded_2718()
}
