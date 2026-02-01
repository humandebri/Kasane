//! どこで: Phase1.6 E2E / 何を: RPC互換メソッドをPocketICで確認 / なぜ: 実環境に近い互換確認のため

use candid::{Decode, Encode, Principal, CandidType};
use candid::Deserialize;
use pocket_ic::PocketIc;
use std::path::PathBuf;

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
    logs: Vec<LogView>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct LogView {
    address: Vec<u8>,
    topics: Vec<Vec<u8>>,
    data: Vec<u8>,
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

fn install_canister(pic: &PocketIc) -> Principal {
    let path = wasm_path();
    if !path.exists() {
        panic!("wasm not found: build ic-evm-wrapper first: {:?}", path);
    }
    let wasm = std::fs::read(path).expect("read wasm");
    let canister_id = pic.create_canister();
    pic.add_cycles(canister_id, 5_000_000_000_000u128);
    pic.install_canister(canister_id, wasm, vec![], None);
    canister_id
}

fn call_query(pic: &PocketIc, canister_id: Principal, method: &str, arg: Vec<u8>) -> Vec<u8> {
    pic.query_call(canister_id, Principal::anonymous(), method, arg)
        .unwrap_or_else(|err| panic!("query error: {err}"))
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
fn rpc_get_transaction_by_hash_and_receipt_return_none() {
    let pic = PocketIc::new();
    let canister_id = install_canister(&pic);

    let tx_hash = vec![0u8; 32];
    let arg = Encode!(&tx_hash).expect("encode");
    let tx_bytes = call_query(&pic, canister_id, "rpc_eth_get_transaction_by_hash", arg);
    let tx: Option<EthTxView> = Decode!(&tx_bytes, Option<EthTxView>).expect("decode tx");
    assert!(tx.is_none());

    let arg = Encode!(&tx_hash).expect("encode");
    let receipt_bytes = call_query(&pic, canister_id, "rpc_eth_get_transaction_receipt", arg);
    let receipt: Option<EthReceiptView> =
        Decode!(&receipt_bytes, Option<EthReceiptView>).expect("decode receipt");
    assert!(receipt.is_none());
}
