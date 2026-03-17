//! どこで: ic-evm-gateway の canbench 導線
//! 何を: submit/produce の最小ベンチマークを提供
//! なぜ: 命令数とメモリ増分の回帰を継続検知するため

use canbench_rs::{bench, bench_fn, BenchResult};
use candid::Principal;
use evm_core::chain;
use evm_core::tx_decode::IcSyntheticTxInput;
use evm_core::tx_decode::{decode_eth_raw_tx, decode_ic_synthetic_header};
use evm_db::stable_state::with_state_mut;
use evm_db::types::keys::make_account_key;
use evm_db::types::values::AccountVal;
use ic_evm_rpc_types::RpcCallObjectView;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

static NONCE_SEQ: AtomicU64 = AtomicU64::new(0);
const UNSUPPORTED_TYPED_4844_PREFIX: [u8; 1] = [0x03];
const ETH_CALL_FROM: [u8; 20] = [0x77u8; 20];
const BENCH_LEGACY_RAW_TX: [u8; 104] = [
    248, 102, 128, 132, 119, 53, 148, 0, 130, 82, 8, 148, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17,
    17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 128, 128, 131, 10, 214, 118, 160, 231, 214, 114, 181,
    71, 43, 129, 98, 169, 65, 80, 181, 239, 81, 253, 32, 8, 31, 223, 49, 210, 20, 22, 11, 183, 240,
    70, 140, 196, 60, 98, 252, 160, 40, 139, 139, 249, 125, 73, 253, 189, 136, 186, 34, 57, 236,
    35, 85, 199, 169, 87, 219, 98, 212, 200, 90, 202, 74, 48, 54, 28, 31, 109, 114, 122,
];

#[bench(raw)]
fn submit_ic_tx_path() -> BenchResult {
    // Warm path: caller principal -> EVM address derivation cache を事前に温め、
    // submit本体のホットパス回帰を継続監視する。
    warm_submit_caller_cache();
    bench_fn(|| {
        let _ = submit_synthetic_tx();
    })
}

#[bench(raw)]
fn submit_ic_tx_path_cold() -> BenchResult {
    bench_fn(|| {
        let _ = submit_synthetic_tx();
    })
}

#[bench(raw)]
fn decode_ic_synthetic_header_path() -> BenchResult {
    let tx = build_ic_tx_bytes(0);
    bench_fn(|| {
        let _ = decode_ic_synthetic_header(&tx);
    })
}

#[bench(raw)]
fn decode_eth_signature_path() -> BenchResult {
    bench_fn(|| {
        let _ = decode_eth_raw_tx(&BENCH_LEGACY_RAW_TX);
    })
}

#[bench(raw)]
fn decode_eth_unsupported_typed_reject_path() -> BenchResult {
    bench_fn(|| {
        let _ = decode_eth_raw_tx(&UNSUPPORTED_TYPED_4844_PREFIX);
    })
}

#[bench(raw)]
fn produce_block_path() -> BenchResult {
    let _ = submit_synthetic_tx();
    bench_fn(|| {
        let _ = chain::produce_block(1);
    })
}

#[bench(raw)]
fn state_root_migration_tick_path() -> BenchResult {
    bench_fn(|| {
        let _ = chain::state_root_migration_tick(1);
    })
}

#[bench(raw)]
fn rpc_eth_call_object_path() -> BenchResult {
    ensure_eth_call_fixture();
    let call = basic_call_object();
    bench_fn(|| {
        let _ = crate::rpc_eth_call_object(call.clone());
    })
}

#[bench(raw)]
fn rpc_eth_call_rawtx_path() -> BenchResult {
    bench_fn(|| {
        let _ = crate::rpc_eth_call_rawtx(BENCH_LEGACY_RAW_TX.to_vec());
    })
}

fn submit_synthetic_tx() -> Result<evm_db::chain_data::TxId, chain::ChainError> {
    let nonce = NONCE_SEQ.fetch_add(1, Ordering::Relaxed);
    let caller = Principal::self_authenticating(b"canbench-caller");
    let canister = Principal::self_authenticating(b"canbench-canister");
    chain::submit_tx_in(chain::TxIn::IcSynthetic {
        caller_principal: caller.as_slice().to_vec(),
        canister_id: canister.as_slice().to_vec(),
        tx: build_ic_tx_input(nonce),
    })
}

fn warm_submit_caller_cache() {
    let caller = Principal::self_authenticating(b"canbench-caller");
    let canister = Principal::self_authenticating(b"canbench-canister");
    let _ = chain::submit_tx_in(chain::TxIn::IcSynthetic {
        caller_principal: caller.as_slice().to_vec(),
        canister_id: canister.as_slice().to_vec(),
        tx: IcSyntheticTxInput {
            to: Some([0u8; 20]),
            value: [0u8; 32],
            gas_limit: 500_000,
            nonce: 0,
            max_fee_per_gas: 2_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            data: Vec::new(),
        },
    });
}

fn build_ic_tx_bytes(nonce: u64) -> Vec<u8> {
    evm_core::tx_decode::encode_ic_synthetic_input(&build_ic_tx_input(nonce))
}

fn build_ic_tx_input(nonce: u64) -> IcSyntheticTxInput {
    IcSyntheticTxInput {
        to: Some([
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
        ]),
        value: [0u8; 32],
        gas_limit: 500_000,
        nonce,
        max_fee_per_gas: 2_000_000_000,
        max_priority_fee_per_gas: 1_000_000_000,
        data: Vec::new(),
    }
}

fn ensure_eth_call_fixture() {
    static INIT: OnceLock<()> = OnceLock::new();
    let _ = INIT.get_or_init(|| {
        with_state_mut(|state| {
            state.accounts.insert(
                make_account_key(ETH_CALL_FROM),
                AccountVal::from_parts(0, [0xffu8; 32], [0u8; 32]),
            );
        });
    });
}

fn basic_call_object() -> RpcCallObjectView {
    RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: Some(ETH_CALL_FROM.to_vec()),
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
