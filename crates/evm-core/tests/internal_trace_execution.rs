//! どこで: evm-core integration tests / 何を: SELFDESTRUCT を含む internal trace 実行経路を検証する / なぜ: inspector 単体だけでなく実 execution でも trace_id 採番回帰を防ぐため

use std::borrow::Cow;

use evm_core::hash;
use evm_core::tx_decode::IcSyntheticTxInput;
use evm_db::chain_data::{InternalTraceActionKind, InternalTraceSet};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};
use evm_db::Storable;

mod common;

fn relax_fee_floor_for_tests() {
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1;
        chain_state.min_gas_price = 1;
        chain_state.min_priority_fee = 1;
        state.chain_state.set(chain_state);
    });
}

fn push_call(target: [u8; 20]) -> Vec<u8> {
    let mut code = Vec::new();
    // CALL(gas, to, value=0, in=0, insize=0, out=0, outsize=0) を最小構成で積む。
    for _ in 0..5 {
        code.extend_from_slice(&[0x60, 0x00]);
    }
    code.push(0x73);
    code.extend_from_slice(&target);
    code.extend_from_slice(&[0x5a, 0xf1, 0x50]);
    code
}

fn selfdestruct_runtime(beneficiary: [u8; 20]) -> Vec<u8> {
    let mut code = vec![0x73];
    code.extend_from_slice(&beneficiary);
    code.push(0xff);
    code
}

fn create_empty_runtime() -> Vec<u8> {
    // memory[0x1b..0x20] に 5 byte の init code を置き、その断片を CREATE する。
    vec![
        0x64, 0x60, 0x00, 0x60, 0x00, 0xf3, 0x60, 0x00, 0x52, 0x60, 0x05, 0x60, 0x1b, 0x60, 0x00,
        0xf0, 0x50,
    ]
}

fn coordinator_runtime(nested_target: [u8; 20], sibling_target: [u8; 20]) -> Vec<u8> {
    // nested CALL -> CREATE -> sibling CALL の順で、同一親配下の trace 採番を観測する。
    let mut code = push_call(nested_target);
    code.extend_from_slice(&create_empty_runtime());
    code.extend_from_slice(&push_call(sibling_target));
    code.push(0x00);
    code
}

#[test]
fn execution_path_keeps_sibling_trace_ids_after_selfdestruct() {
    init_stable_state();
    relax_fee_floor_for_tests();

    let caller_principal = vec![0x91];
    let caller = hash::derive_evm_address_from_principal(&caller_principal).expect("must derive");
    common::fund_account(caller, 1_000_000_000_000_000_000);

    let beneficiary = [0x55; 20];
    let orchestrator = [0x10; 20];
    let coordinator = [0x20; 20];
    let selfdestruct_target = [0x30; 20];
    let sibling_target = [0x40; 20];

    common::install_contract(selfdestruct_target, &selfdestruct_runtime(beneficiary));
    common::install_contract(sibling_target, &[0x00]);
    common::install_contract(
        coordinator,
        &coordinator_runtime(selfdestruct_target, sibling_target),
    );
    common::install_contract(orchestrator, &push_call(coordinator));

    let (tx_id, receipt) = common::execute_ic_tx_via_produce(
        caller_principal,
        vec![0xaa],
        IcSyntheticTxInput {
            to: Some(orchestrator),
            value: [0u8; 32],
            gas_limit: 300_000,
            nonce: 0,
            max_fee_per_gas: 2_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            data: Vec::new(),
        },
    );
    assert_eq!(receipt.status, 1);

    let traces = with_state(|state| {
        let ptr = state
            .internal_traces
            .get(&tx_id)
            .expect("internal traces must be stored");
        let bytes = state
            .blob_store
            .read(&ptr)
            .expect("internal trace bytes must be readable");
        InternalTraceSet::from_bytes(Cow::Owned(bytes))
    });

    assert_eq!(traces.total_count, 5);
    assert_eq!(traces.items.len(), 5);

    assert_eq!(traces.items[0].trace_id, "0");
    assert_eq!(traces.items[0].action_kind, InternalTraceActionKind::Call);
    assert_eq!(traces.items[0].to_address, Some(coordinator));

    assert_eq!(traces.items[1].trace_id, "0_0");
    assert_eq!(traces.items[1].action_kind, InternalTraceActionKind::Call);
    assert_eq!(traces.items[1].to_address, Some(selfdestruct_target));

    assert_eq!(traces.items[2].trace_id, "0_0_0");
    assert_eq!(
        traces.items[2].action_kind,
        InternalTraceActionKind::Selfdestruct
    );
    assert_eq!(traces.items[2].to_address, Some(beneficiary));

    assert_eq!(traces.items[3].trace_id, "0_1");
    assert_eq!(traces.items[3].action_kind, InternalTraceActionKind::Create);
    assert!(traces.items[3].created_contract_address.is_some());

    assert_eq!(traces.items[4].trace_id, "0_2");
    assert_eq!(traces.items[4].action_kind, InternalTraceActionKind::Call);
    assert_eq!(traces.items[4].to_address, Some(sibling_target));
}
