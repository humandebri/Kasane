//! どこで: ic-evm-wrapper の canbench 導線
//! 何を: submit/produce の最小ベンチマークを提供
//! なぜ: 命令数とメモリ増分の回帰を継続検知するため

use canbench_rs::{bench, bench_fn, BenchResult};
use candid::Principal;
use evm_core::chain;
use std::sync::atomic::{AtomicU64, Ordering};

static NONCE_SEQ: AtomicU64 = AtomicU64::new(0);

#[bench(raw)]
fn submit_ic_tx_path() -> BenchResult {
    bench_fn(|| {
        let _ = submit_synthetic_tx();
    })
}

#[bench(raw)]
fn produce_block_path() -> BenchResult {
    let _ = submit_synthetic_tx();
    bench_fn(|| {
        let _ = chain::produce_block(1);
    })
}

fn submit_synthetic_tx() -> Result<evm_db::chain_data::TxId, chain::ChainError> {
    let nonce = NONCE_SEQ.fetch_add(1, Ordering::Relaxed);
    let caller = Principal::self_authenticating(b"canbench-caller");
    let canister = Principal::self_authenticating(b"canbench-canister");
    chain::submit_tx_in(chain::TxIn::IcSynthetic {
        caller_principal: caller.as_slice().to_vec(),
        canister_id: canister.as_slice().to_vec(),
        tx_bytes: build_ic_tx_bytes(nonce),
    })
}

fn build_ic_tx_bytes(nonce: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(105);
    out.push(0x02);
    out.extend_from_slice(&[
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
    ]);
    out.extend_from_slice(&[0u8; 32]);
    out.extend_from_slice(&500_000u64.to_be_bytes());
    out.extend_from_slice(&nonce.to_be_bytes());
    out.extend_from_slice(&2_000_000_000u128.to_be_bytes());
    out.extend_from_slice(&1_000_000_000u128.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}
