//! Transactions that read an external price must seal against the reserved EVM state.
use evm_core::{
    chain::{self, CallObjectInput, ChainError},
    hash,
    kasane_precompiles::{precompile_allow_key, ICP_QUERY_PRECOMPILE_ADDRESS},
    tx_decode::IcSyntheticTxInput,
};
use evm_db::{
    chain_data::{
        constants::{CHAIN_ID, MAX_RETURN_DATA},
        QueryTxPhase, RuntimeConfigV1, TxId,
    },
    stable_state::{init_stable_state, set_runtime_config, with_state, with_state_mut},
    types::keys::{make_account_key, make_storage_key},
};
use revm::primitives::U256;
mod common;
const CONTRACT: [u8; 20] = [0x76; 20];
fn input() -> Vec<u8> {
    let mut b = vec![1, 0, 1, 1, 5];
    b.extend_from_slice(b"price");
    b.extend_from_slice(&0u32.to_be_bytes());
    b
}
fn setup() -> ([u8; 20], Vec<u8>) {
    init_stable_state();
    set_runtime_config(RuntimeConfigV1::new(
        candid::Principal::self_authenticating(b"query-tx-wrap"),
        [0x55; 20],
    ));
    with_state_mut(|s| {
        let mut c = *s.chain_state.get();
        c.base_fee = 1;
        c.min_gas_price = 1;
        c.min_priority_fee = 1;
        s.chain_state.set(c);
        s.tx_query_precompile_allowlist
            .insert(precompile_allow_key(&[1], "price"), 1);
    });
    let principal = vec![0x41];
    let caller = hash::derive_evm_address_from_principal(&principal).unwrap();
    common::fund_account(caller, 1_000_000_000_000_000_000);
    common::install_contract(CONTRACT, &price_runtime(false));
    (caller, principal)
}
fn price_runtime(catch: bool) -> Vec<u8> {
    // Copy request, CALL precompile, and persist the returned price. All changes are journaled.
    let mut b = vec![
        0x36, 0x5f, 0x5f, 0x37, 0x60, 0x20, 0x5f, 0x36, 0x5f, 0x5f, 0x73,
    ];
    b.extend_from_slice(ICP_QUERY_PRECOMPILE_ADDRESS.as_slice());
    b.extend_from_slice(&[0x62, 0x01, 0x86, 0xa0, 0xf1]);
    if catch {
        b.extend_from_slice(&[0x50, 0x60, 0x07, 0x5f, 0x55, 0x00]);
    } else {
        b.extend_from_slice(&[0x15, 0x60, 0, 0x57, 0x5f, 0x51, 0x5f, 0x55, 0x00]);
        let dest = b.len() as u8;
        let pos = b.len() - 7;
        b[pos] = dest;
        b.extend_from_slice(&[0x5b, 0x5f, 0x5f, 0xfd]);
    }
    b
}
fn tx(principal: Vec<u8>, to: [u8; 20], nonce: u64, priority: u128, data: Vec<u8>) -> TxId {
    chain::submit_ic_tx_input(
        principal,
        vec![0xa0],
        IcSyntheticTxInput {
            to: Some(to),
            value: [0; 32],
            gas_limit: 300_000,
            nonce,
            max_fee_per_gas: 10,
            max_priority_fee_per_gas: priority,
            data,
        },
    )
    .unwrap()
}
fn slot() -> U256 {
    with_state(|s| {
        s.storage
            .get(&make_storage_key(CONTRACT, [0; 32]))
            .map(|v| U256::from_be_bytes(v.0))
            .unwrap_or_default()
    })
}
fn nonce(caller: [u8; 20]) -> u64 {
    with_state(|s| s.accounts.get(&make_account_key(caller)).unwrap().nonce())
}
fn reply(price: u8) -> Vec<u8> {
    let mut b = vec![0; 32];
    b[31] = price;
    b
}
fn complete(value: Result<Vec<u8>, String>) {
    let j = chain::pending_query_tx().unwrap();
    assert!(chain::start_query_tx_call(j.attempt_id));
    assert!(chain::finish_query_tx_call(
        j.attempt_id,
        j.started_at + 1,
        value
    ));
}
#[test]
fn query_tx_price_updates_storage_and_blocks_other_transactions() {
    let (caller, p) = setup();
    let next_principal = vec![0x42];
    let next_caller = hash::derive_evm_address_from_principal(&next_principal).unwrap();
    common::fund_account(next_caller, 1_000_000_000_000_000_000);
    let id = tx(p.clone(), CONTRACT, 0, 2, input());
    assert_eq!(
        chain::produce_block(10).unwrap_err(),
        ChainError::QueryTxBusy
    );
    let j = chain::pending_query_tx().unwrap();
    assert_eq!(j.phase, QueryTxPhase::Waiting);
    assert_eq!(nonce(caller), 0);
    assert_eq!(slot(), U256::ZERO);
    assert!(chain::get_receipt(&id).is_none());
    let after = tx(next_principal, [0x99; 20], 0, 9, vec![]);
    assert_eq!(
        chain::credit_balance(caller, 1),
        Err(ChainError::QueryTxBusy)
    );
    assert_eq!(
        chain::credit_native_deposit([0x88; 32], caller, reply(1).try_into().unwrap()),
        Err(ChainError::QueryTxBusy)
    );
    assert_eq!(
        chain::produce_block(10).unwrap_err(),
        ChainError::QueryTxBusy
    );
    complete(Ok(reply(42)));
    let block = chain::produce_block(10).unwrap();
    assert_eq!(block.block.tx_ids, vec![id]);
    assert_eq!(block.block.number, j.block_number);
    assert_eq!(block.block.timestamp, j.timestamp);
    assert_eq!(chain::get_receipt(&id).unwrap().status, 1);
    assert_eq!(nonce(caller), 1);
    assert_eq!(slot(), U256::from(42));
    assert!(chain::pending_query_tx().is_none());
    assert_eq!(chain::produce_block(10).unwrap().block.tx_ids, vec![after]);
    assert!(!chain::finish_query_tx_call(
        j.attempt_id,
        j.started_at + 2,
        Ok(reply(99))
    ));
    assert_eq!(slot(), U256::from(42));
}
#[test]
fn query_tx_prefix_is_committed_before_reserving_query() {
    let (_, p) = setup();
    let other = vec![0x42];
    let a = hash::derive_evm_address_from_principal(&other).unwrap();
    common::fund_account(a, 1_000_000_000_000_000_000);
    let before = tx(other, [0x99; 20], 0, 3, vec![]);
    let id = tx(p, CONTRACT, 0, 2, input());
    assert_eq!(chain::produce_block(10).unwrap().block.tx_ids, vec![before]);
    let reserved = chain::pending_query_tx().unwrap();
    assert_eq!(reserved.phase, QueryTxPhase::Reserved);
    assert_eq!(
        chain::produce_block(10).unwrap_err(),
        ChainError::QueryTxBusy
    );
    let waiting = chain::pending_query_tx().unwrap();
    assert_eq!(waiting.block_number, reserved.block_number);
    assert_eq!(waiting.timestamp, reserved.timestamp);
    assert_eq!(waiting.base_fee, reserved.base_fee);
    assert_eq!(waiting.attempt_id, reserved.attempt_id);
    complete(Ok(reply(12)));
    assert_eq!(chain::produce_block(10).unwrap().block.tx_ids, vec![id]);
    assert_eq!(slot(), U256::from(12));
}
#[test]
fn query_tx_after_prefix_uses_updated_base_fee() {
    let (caller, p) = setup();
    with_state_mut(|s| {
        let mut c = *s.chain_state.get();
        c.base_fee = 1_000;
        s.chain_state.set(c);
    });
    let other = vec![0x42];
    let a = hash::derive_evm_address_from_principal(&other).unwrap();
    common::fund_account(a, 1_000_000_000_000_000_000);
    let submit = |principal, to, priority, data| {
        chain::submit_ic_tx_input(
            principal,
            vec![0xa0],
            IcSyntheticTxInput {
                to: Some(to),
                value: [0; 32],
                gas_limit: 300_000,
                nonce: 0,
                max_fee_per_gas: 10_000,
                max_priority_fee_per_gas: priority,
                data,
            },
        )
        .unwrap()
    };
    let before = submit(other, [0x99; 20], 3, vec![]);
    let id = submit(p, CONTRACT, 2, input());
    let prefix = chain::produce_block(10).unwrap();
    assert_eq!(prefix.block.tx_ids, vec![before]);
    let next_fee = evm_core::base_fee::compute_next_base_fee(
        prefix.block.base_fee_per_gas,
        prefix.gas_used,
        prefix.block.block_gas_limit,
    );
    assert_ne!(next_fee, prefix.block.base_fee_per_gas);
    let reserved = chain::pending_query_tx().unwrap();
    assert_eq!(reserved.base_fee, next_fee);
    assert_eq!(
        chain::produce_block(10).unwrap_err(),
        ChainError::QueryTxBusy
    );
    assert_eq!(chain::pending_query_tx().unwrap().base_fee, next_fee);
    let balance_before =
        with_state(|s| s.accounts.get(&make_account_key(caller)).unwrap().balance());
    complete(Ok(reply(42)));
    let query_block = chain::produce_block(10).unwrap();
    assert_eq!(query_block.block.tx_ids, vec![id]);
    assert_eq!(query_block.block.base_fee_per_gas, next_fee);
    let receipt = chain::get_receipt(&id).unwrap();
    assert_eq!(receipt.status, 1);
    assert_eq!(receipt.effective_gas_price, next_fee + 2);
    let balance_after =
        with_state(|s| s.accounts.get(&make_account_key(caller)).unwrap().balance());
    assert_eq!(
        U256::from_be_bytes(balance_before) - U256::from_be_bytes(balance_after),
        U256::from(receipt.gas_used) * U256::from(receipt.effective_gas_price)
    );
    assert_eq!(slot(), U256::from(42));
}

#[test]
fn query_tx_timeout_revert_charges_once_and_releases_reservation() {
    let (caller, p) = setup();
    let id = tx(p, CONTRACT, 0, 2, input());
    let before = with_state(|s| s.accounts.get(&make_account_key(caller)).unwrap().balance());
    assert_eq!(
        chain::produce_block(1).unwrap_err(),
        ChainError::QueryTxBusy
    );
    let j = chain::pending_query_tx().unwrap();
    assert!(!chain::expire_query_tx(j.deadline - 1));
    assert!(chain::expire_query_tx(j.deadline));
    chain::produce_block(1).unwrap();
    let r = chain::get_receipt(&id).unwrap();
    assert_eq!(r.status, 0);
    assert_eq!(nonce(caller), 1);
    assert_eq!(slot(), U256::ZERO);
    let balance = with_state(|s| s.accounts.get(&make_account_key(caller)).unwrap().balance());
    assert_eq!(
        U256::from_be_bytes(before) - U256::from_be_bytes(balance),
        U256::from(r.gas_used) * U256::from(r.effective_gas_price)
    );
    assert!(!chain::finish_query_tx_call(
        j.attempt_id,
        j.deadline + 1,
        Ok(reply(1))
    ));
}
#[test]
fn query_tx_error_can_be_caught_by_contract() {
    let (_, p) = setup();
    common::install_contract(CONTRACT, &price_runtime(true));
    let id = tx(p, CONTRACT, 0, 2, input());
    assert_eq!(
        chain::produce_block(1).unwrap_err(),
        ChainError::QueryTxBusy
    );
    complete(Err("ic_query.call_failed".into()));
    chain::produce_block(1).unwrap();
    assert_eq!(chain::get_receipt(&id).unwrap().status, 1);
    assert_eq!(slot(), U256::from(7));
}
fn finish_error_case(interrupted: bool) {
    let (_, p) = setup();
    let id = tx(p, CONTRACT, 0, 2, input());
    assert_eq!(
        chain::produce_block(1).unwrap_err(),
        ChainError::QueryTxBusy
    );
    if interrupted {
        chain::interrupt_query_tx_after_upgrade();
    } else {
        complete(Ok(vec![0; MAX_RETURN_DATA + 1]));
    }
    assert_eq!(
        chain::pending_query_tx().unwrap().phase,
        QueryTxPhase::Ready
    );
    chain::produce_block(1).unwrap();
    assert_eq!(chain::get_receipt(&id).unwrap().status, 0);
    assert!(chain::pending_query_tx().is_none());
}
#[test]
fn query_tx_oversize_does_not_reissue() {
    finish_error_case(false);
}
#[test]
fn query_tx_upgrade_does_not_reissue() {
    finish_error_case(true);
}
#[test]
fn query_tx_estimation_reads_once_without_state_changes() {
    let (caller, _) = setup();
    let count = std::cell::Cell::new(0);
    let call = CallObjectInput {
        to: Some(CONTRACT),
        from: caller,
        gas_limit: Some(300_000),
        gas_price: Some(10),
        nonce: Some(0),
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: Some(CHAIN_ID),
        tx_type: None,
        access_list: vec![],
        value: [0; 32],
        data: input(),
    };
    let gas = common::run_ready_future(chain::eth_estimate_gas_object_async(call, |_| {
        count.set(count.get() + 1);
        async { Ok(reply(42)) }
    }))
    .unwrap();
    assert!(gas > 50_000 && gas < 300_000);
    assert_eq!(count.get(), 1);
    assert_eq!(slot(), U256::ZERO);
    assert_eq!(nonce(caller), 0);
    assert!(chain::pending_query_tx().is_none());
}
#[test]
fn query_tx_cannot_be_replaced_while_waiting() {
    let (_, p) = setup();
    tx(p.clone(), CONTRACT, 0, 2, input());
    assert_eq!(
        chain::produce_block(1).unwrap_err(),
        ChainError::QueryTxBusy
    );
    let result = chain::submit_ic_tx_input(
        p,
        vec![0xa0],
        IcSyntheticTxInput {
            to: Some([0x99; 20]),
            value: [0; 32],
            gas_limit: 300_000,
            nonce: 0,
            max_fee_per_gas: 20,
            max_priority_fee_per_gas: 10,
            data: vec![],
        },
    );
    assert_eq!(result, Err(ChainError::QueryTxBusy));
}

#[test]
fn query_tx_unlisted_target_fails_without_reservation() {
    let (_, p) = setup();
    with_state_mut(|s| {
        s.tx_query_precompile_allowlist
            .remove(&precompile_allow_key(&[1], "price"));
    });
    let id = tx(p, CONTRACT, 0, 2, input());
    chain::produce_block(1).unwrap();
    assert_eq!(chain::get_receipt(&id).unwrap().status, 0);
    assert!(chain::pending_query_tx().is_none());
}
#[test]
fn query_tx_changed_snapshot_drops_without_charging_or_advancing_nonce() {
    let (caller, p) = setup();
    let id = tx(p, CONTRACT, 0, 2, input());
    assert_eq!(
        chain::produce_block(1).unwrap_err(),
        ChainError::QueryTxBusy
    );
    complete(Ok(reply(42)));
    with_state_mut(|s| {
        let mut c = *s.chain_state.get();
        c.min_gas_price += 1;
        s.chain_state.set(c);
    });
    assert_eq!(
        chain::produce_block(1).unwrap_err(),
        ChainError::NoExecutableTx
    );
    assert_eq!(nonce(caller), 0);
    assert!(chain::get_receipt(&id).is_none());
    assert!(chain::pending_query_tx().is_none());
}
#[test]
fn query_tx_expired_success_is_not_applied_after_pause() {
    let (_, p) = setup();
    let id = tx(p, CONTRACT, 0, 2, input());
    assert_eq!(
        chain::produce_block(1).unwrap_err(),
        ChainError::QueryTxBusy
    );
    complete(Ok(reply(42)));
    let job = chain::pending_query_tx().unwrap();
    assert!(chain::expire_query_tx(job.deadline));
    chain::produce_block(1).unwrap();
    assert_eq!(chain::get_receipt(&id).unwrap().status, 0);
    assert_eq!(slot(), U256::ZERO);
}

#[test]
fn query_tx_failed_seal_does_not_clear_saved_response() {
    let (_, p) = setup();
    let id = tx(p, CONTRACT, 0, 2, input());
    assert_eq!(
        chain::produce_block(1).unwrap_err(),
        ChainError::QueryTxBusy
    );
    complete(Ok(reply(42)));
    chain::configure_store_failpoint_for_test(Some(1));
    let trapped = std::panic::catch_unwind(|| {
        let _ = chain::produce_block(1);
    });
    chain::configure_store_failpoint_for_test(None);
    assert!(trapped.is_err());
    // Host stable memory does not emulate IC message rollback; verify the clear happens after persistence.
    assert_eq!(
        chain::pending_query_tx().unwrap().phase,
        QueryTxPhase::Ready
    );
    assert!(chain::get_receipt(&id).is_none());
}

#[test]
fn query_tx_second_query_is_rejected_without_another_dispatch() {
    let (_, p) = setup();
    let runtime = price_runtime(false);
    let end = runtime.iter().position(|b| *b == 0xf1).unwrap() + 1;
    let mut twice = runtime[..end].to_vec();
    twice.push(0x50);
    twice.extend_from_slice(&runtime[..end]);
    twice.extend_from_slice(&[0x5f, 0x55, 0x00]);
    common::install_contract(CONTRACT, &twice);
    let id = tx(p, CONTRACT, 0, 2, input());
    assert_eq!(
        chain::produce_block(1).unwrap_err(),
        ChainError::QueryTxBusy
    );
    complete(Ok(reply(42)));
    chain::produce_block(1).unwrap();
    assert_eq!(chain::get_receipt(&id).unwrap().status, 1);
    assert_eq!(slot(), U256::ZERO);
    assert!(chain::pending_query_tx().is_none());
}
#[test]
fn query_tx_minimum_precompile_gas_checked_before_dispatch() {
    let (_, p) = setup();
    let mut runtime = price_runtime(false);
    let pos = runtime
        .windows(4)
        .position(|b| b == [0x62, 0x01, 0x86, 0xa0])
        .unwrap();
    runtime[pos + 1..pos + 4].copy_from_slice(&[0, 0, 1]);
    common::install_contract(CONTRACT, &runtime);
    let id = tx(p, CONTRACT, 0, 2, input());
    chain::produce_block(1).unwrap();
    assert_eq!(chain::get_receipt(&id).unwrap().status, 0);
    assert!(chain::pending_query_tx().is_none());
}
#[test]
fn query_estimation_request_changes_fail_explicitly_without_second_read() {
    let (caller, _) = setup();
    let mut data = input();
    let n = data.len();
    data[n - 1] = 1;
    data.push(0);
    let runtime = price_runtime(false);
    let mut varying = runtime[..4].to_vec();
    varying.extend_from_slice(&[0x5a, 0x60, (data.len() - 1) as u8, 0x53]);
    varying.extend_from_slice(&runtime[4..]);
    let jump = varying.iter().position(|b| *b == 0x57).unwrap();
    varying[jump - 1] += 4;
    common::install_contract(CONTRACT, &varying);
    let count = std::cell::Cell::new(0);
    let call = CallObjectInput {
        to: Some(CONTRACT),
        from: caller,
        gas_limit: Some(300_000),
        gas_price: Some(10),
        nonce: Some(0),
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: Some(CHAIN_ID),
        tx_type: None,
        access_list: vec![],
        value: [0; 32],
        data,
    };
    let out = common::run_ready_future(chain::eth_estimate_gas_object_async(call, |_| {
        count.set(count.get() + 1);
        async { Ok(reply(42)) }
    }));
    assert!(matches!(
        out,
        Err(ChainError::ExecFailed(Some(
            evm_core::revm_exec::ExecError::ExternalQuery(_)
        )))
    ));
    assert_eq!(count.get(), 1);
}
