//! Actual consensus call: raw Candid price drives storage and a transfer in one EVM tx.
use candid::{CandidType, Decode, Deserialize, Encode, Nat, Principal};
use evm_core::{hash, kasane_precompiles::ICP_QUERY_PRECOMPILE_ADDRESS};
use pocket_ic::{PocketIc, PocketIcBuilder};
use std::{path::PathBuf, time::Duration};
#[derive(CandidType)]
struct Balance {
    address: Vec<u8>,
    amount: u128,
}
#[derive(CandidType)]
struct Init {
    genesis_balances: Vec<Balance>,
    wrap_canister_id: Principal,
    wrap_factory_address: Vec<u8>,
    query_instruction_soft_limit: Option<u64>,
    update_instruction_soft_limit: Option<u64>,
}
#[derive(CandidType)]
struct Allow {
    target: Principal,
    method: String,
}
#[derive(CandidType)]
struct Tx {
    to: Option<Vec<u8>>,
    from: Option<Vec<u8>>,
    value: Nat,
    max_priority_fee_per_gas: Nat,
    data: Vec<u8>,
    max_fee_per_gas: Nat,
    nonce: u64,
    gas_limit: u64,
}
#[derive(Debug, CandidType, Deserialize)]
enum SubmitError {
    InvalidArgument(String),
    Rejected(String),
    Internal(String),
}
#[derive(Debug, CandidType, Deserialize)]
enum Lookup {
    NotFound,
    Pending,
    Pruned { pruned_before_block: u64 },
}
#[derive(Debug, CandidType, Deserialize)]
struct Receipt {
    status: u8,
    block_number: u64,
    contract_address: Option<Vec<u8>>,
}
#[derive(CandidType, Deserialize)]
struct Pending {
    attempt_id: u64,
}
#[derive(CandidType)]
struct EstimateCall {
    to: Option<Vec<u8>>,
    from: Option<Vec<u8>>,
    gas: Option<u64>,
    value: Option<Vec<u8>>,
    data: Option<Vec<u8>>,
}
#[derive(CandidType)]
enum Tag {
    Latest,
}
#[derive(Debug, CandidType, Deserialize)]
struct RpcError {
    code: u32,
    message: String,
    error_prefix: Option<String>,
}
fn wasm(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/wasm32-unknown-unknown/release")
            .join(name),
    )
    .expect("build Wasm fixtures first")
}
fn caller() -> Principal {
    Principal::self_authenticating(b"query-tx-e2e")
}
fn query(pic: &PocketIc, id: Principal, method: &str, args: Vec<u8>) -> Vec<u8> {
    pic.query_call(id, caller(), method, args).unwrap()
}
fn submit(
    pic: &PocketIc,
    id: Principal,
    to: Option<Vec<u8>>,
    nonce: u64,
    data: Vec<u8>,
    value: u64,
) -> Vec<u8> {
    submit_as(pic, id, caller(), to, nonce, data, value)
}
fn submit_as(
    pic: &PocketIc,
    id: Principal,
    sender: Principal,
    to: Option<Vec<u8>>,
    nonce: u64,
    data: Vec<u8>,
    value: u64,
) -> Vec<u8> {
    let args = Tx {
        to,
        from: None,
        value: value.into(),
        max_priority_fee_per_gas: 300_000_000_000u64.into(),
        max_fee_per_gas: 600_000_000_000u64.into(),
        nonce,
        data,
        gas_limit: 300_000,
    };
    let bytes = pic
        .update_call(id, sender, "submit_ic_tx", Encode!(&args).unwrap())
        .unwrap();
    Decode!(&bytes, Result<Vec<u8>, SubmitError>)
        .unwrap()
        .unwrap()
}
fn receipt(pic: &PocketIc, id: Principal, tx: Vec<u8>) -> Receipt {
    for _ in 0..2000 {
        pic.advance_time(Duration::from_millis(10));
        pic.tick();
        let bytes = query(pic, id, "get_receipt", Encode!(&tx).unwrap());
        match Decode!(&bytes,Result<Receipt,Lookup>).unwrap() {
            Ok(r) => return r,
            Err(Lookup::Pending | Lookup::NotFound) => {}
            Err(e) => panic!("{e:?}"),
        }
    }
    panic!("transaction did not finish within fixture ticks")
}
fn runtime(seller: [u8; 20]) -> Vec<u8> {
    let mut b = vec![
        0x36, 0x5f, 0x5f, 0x37, 0x60, 0x20, 0x5f, 0x36, 0x5f, 0x5f, 0x73,
    ];
    b.extend_from_slice(ICP_QUERY_PRECOMPILE_ADDRESS.as_slice());
    b.extend_from_slice(&[0x62, 0x01, 0x86, 0xa0, 0xf1, 0x15, 0x61, 0, 0, 0x57]);
    let patch = b.len() - 3;
    // A nat64 Candid reply has a 7-byte header followed by little-endian price.
    b.push(0x5f);
    for i in 0..8u8 {
        b.extend_from_slice(&[0x60, 7 + i, 0x51, 0x60, 248, 0x1c]);
        if i > 0 {
            b.extend_from_slice(&[0x60, i * 8, 0x1b]);
        }
        b.push(0x17);
    }
    b.extend_from_slice(&[
        0x80, 0x5f, 0x55, 0x60, 0x40, 0x52, 0x5f, 0x5f, 0x5f, 0x5f, 0x60, 0x40, 0x51, 0x73,
    ]);
    b.extend_from_slice(&seller);
    b.extend_from_slice(&[0x61, 0xff, 0xff, 0xf1, 0x15, 0x61, 0, 0, 0x57, 0x00]);
    let patch2 = b.len() - 4;
    let dest = (b.len() as u16).to_be_bytes();
    b[patch..patch + 2].copy_from_slice(&dest);
    b[patch2..patch2 + 2].copy_from_slice(&dest);
    b.extend_from_slice(&[0x5b, 0x5f, 0x5f, 0xfd]);
    b
}
fn init_code(runtime: Vec<u8>) -> Vec<u8> {
    let len = (runtime.len() as u16).to_be_bytes();
    let mut b = vec![
        0x61, len[0], len[1], 0x61, 0, 15, 0x60, 0, 0x39, 0x61, len[0], len[1], 0x60, 0, 0xf3,
    ];
    b.extend(runtime);
    b
}
fn price_trade(cross_subnet: bool, interrupt: bool) {
    let pic = PocketIcBuilder::new()
        .with_application_subnet()
        .with_application_subnet()
        .build();
    let subnets = pic.topology().get_app_subnets();
    let oracle = pic.create_canister_on_subnet(None, None, subnets[0]);
    pic.add_cycles(oracle, 5_000_000_000_000);
    pic.install_canister(
        oracle,
        wasm("examples/query_price_oracle.wasm"),
        Encode!().unwrap(),
        None,
    );
    assert_eq!(
        Decode!(&query(&pic, oracle, "price", Encode!().unwrap()), u64).unwrap(),
        41
    );
    let id = pic.create_canister_on_subnet(None, None, subnets[usize::from(cross_subnet)]);
    pic.add_cycles(id, 50_000_000_000_000);
    let init = Some(Init {
        genesis_balances: vec![
            Balance {
                address: hash::derive_evm_address_from_principal(caller().as_slice())
                    .unwrap()
                    .to_vec(),
                amount: 10_000_000_000_000_000_000,
            },
            Balance {
                address: hash::derive_evm_address_from_principal(
                    Principal::self_authenticating(b"query-upgrade-trader").as_slice(),
                )
                .unwrap()
                .to_vec(),
                amount: 10_000_000_000_000_000_000,
            },
        ],
        wrap_canister_id: oracle,
        wrap_factory_address: vec![0x90; 20],
        query_instruction_soft_limit: None,
        update_instruction_soft_limit: None,
    });
    pic.install_canister(
        id,
        wasm("ic_evm_gateway.wasm"),
        Encode!(&init).unwrap(),
        None,
    );
    pic.set_controllers(id, None, vec![caller()]).unwrap();
    for _ in 0..6 {
        pic.advance_time(Duration::from_secs(60));
        pic.tick();
    }
    let bytes = pic
        .update_call(
            id,
            caller(),
            "add_tx_query_precompile_allowed_method",
            Encode!(&Allow {
                target: oracle,
                method: "price".into()
            })
            .unwrap(),
        )
        .unwrap();
    Decode!(&bytes,Result<(),String>).unwrap().unwrap();
    let seller = [0x98; 20];
    let deployment = receipt(
        &pic,
        id,
        submit(&pic, id, None, 0, init_code(runtime(seller)), 0),
    );
    assert_eq!(deployment.status, 1);
    let contract = deployment.contract_address.unwrap();
    let arg = Encode!().unwrap();
    let mut request = vec![1, 0, oracle.as_slice().len() as u8];
    request.extend(oracle.as_slice());
    request.push(5);
    request.extend(b"price");
    request.extend((arg.len() as u32).to_be_bytes());
    request.extend(arg);
    let mut value = vec![0u8; 32];
    value[31] = 100;
    let estimate = EstimateCall {
        to: Some(contract.clone()),
        from: Some(
            hash::derive_evm_address_from_principal(caller().as_slice())
                .unwrap()
                .to_vec(),
        ),
        gas: Some(300_000),
        value: Some(value),
        data: Some(request.clone()),
    };
    let bytes = query(
        &pic,
        id,
        "rpc_eth_estimate_gas_object_at_with_query_precompile",
        Encode!(&estimate, &Tag::Latest).unwrap(),
    );
    let estimate = Decode!(&bytes, Result<u64, RpcError>).unwrap();
    if cross_subnet {
        // IC composite query routing cannot read a remote subnet; the replicated tx below can.
        assert!(estimate.is_err());
    } else {
        let gas = estimate.unwrap();
        assert!(gas > 50_000 && gas < 300_000);
    }
    let trade_id = if interrupt {
        submit(&pic, id, Some(vec![0x99; 20]), 1, vec![], 0);
        submit_as(
            &pic,
            id,
            Principal::self_authenticating(b"query-upgrade-trader"),
            Some(contract.clone()),
            0,
            request,
            100,
        )
    } else {
        submit(&pic, id, Some(contract.clone()), 1, request, 100)
    };
    if interrupt {
        let mut pending = false;
        for _ in 0..400 {
            pic.advance_time(Duration::from_millis(10));
            pic.tick();
            let bytes = query(&pic, id, "get_pending_query_tx", Encode!().unwrap());
            if Decode!(&bytes, Option<Pending>).unwrap().is_some() {
                pending = true;
                break;
            }
        }
        assert!(pending, "must upgrade during a reserved session");
        pic.upgrade_canister(
            id,
            wasm("ic_evm_gateway.wasm"),
            Encode!(&init).unwrap(),
            Some(caller()),
        )
        .unwrap();
    }
    if interrupt {
        let bytes = query(&pic, id, "get_pending_query_tx", Encode!().unwrap());
        assert!(Decode!(&bytes, Option<Pending>).unwrap().is_none());
        let bytes = query(&pic, id, "get_receipt", Encode!(&trade_id).unwrap());
        assert!(Decode!(&bytes,Result<Receipt,Lookup>).unwrap().is_err());
        let sender = hash::derive_evm_address_from_principal(
            Principal::self_authenticating(b"query-upgrade-trader").as_slice(),
        )
        .unwrap()
        .to_vec();
        let bytes = query(
            &pic,
            id,
            "rpc_eth_get_balance",
            Encode!(&sender, &Tag::Latest).unwrap(),
        );
        let balance = Decode!(&bytes, Result<Vec<u8>, RpcError>).unwrap().unwrap();
        assert_eq!(
            u128::from_be_bytes(balance[16..].try_into().unwrap()),
            10_000_000_000_000_000_000
        );
    } else {
        let trade = receipt(&pic, id, trade_id);
        assert_eq!(trade.status, 1);
        assert!(trade.block_number > deployment.block_number);
    }
    let bytes = query(
        &pic,
        id,
        "rpc_eth_get_storage_at",
        Encode!(&contract, &vec![0u8; 32], &Tag::Latest).unwrap(),
    );
    let slot = Decode!(&bytes, Result<Vec<u8>, RpcError>).unwrap().unwrap();
    assert_eq!(slot[31], if interrupt { 0 } else { 42 });
    assert!(slot[..31].iter().all(|b| *b == 0));
    let bytes = query(
        &pic,
        id,
        "rpc_eth_get_balance",
        Encode!(&seller.to_vec(), &Tag::Latest).unwrap(),
    );
    let balance = Decode!(&bytes, Result<Vec<u8>, RpcError>).unwrap().unwrap();
    assert_eq!(balance[31], if interrupt { 0 } else { 42 });
    let bytes = query(
        &pic,
        id,
        "rpc_eth_get_balance",
        Encode!(&contract, &Tag::Latest).unwrap(),
    );
    let balance = Decode!(&bytes, Result<Vec<u8>, RpcError>).unwrap().unwrap();
    assert_eq!(balance[31], if interrupt { 0 } else { 58 });
}

#[test]
fn replicated_price_query_drives_storage_and_payment_in_same_transaction() {
    price_trade(false, false);
}
#[test]
fn cross_subnet_price_query_drives_storage_and_payment() {
    price_trade(true, false);
}

#[test]
fn upgrade_drops_reserved_price_query_without_payment() {
    price_trade(false, true);
}
