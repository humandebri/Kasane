use super::{
    allowance_slot, approval_event_topic0, compute_asset_key, compute_extra_gas,
    estimate_wrap_precompile_gas, extra_gas_by_instruction_ratio, extra_gas_for_precompile,
    icp_update_intent_event_topic0, icp_update_intent_from_log, native_value_to_e8s,
    native_withdraw_event_topic0, native_withdraw_intent_from_log, parse_icp_query_input,
    parse_icp_update_intent_input, parse_input, topic_from_address, transfer_event_topic0,
    unwrap_intent_from_log, unwrap_owner, wrap_event_topic0, COMPACT_ICP_PRECOMPILE_FORMAT_VERSION,
    COMPACT_UNWRAP_FORMAT_VERSION, ICP_PRECOMPILE_KIND_UPDATE, ICP_QUERY_KIND_QUERY,
    ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS, MAX_PRINCIPAL_LEN, NATIVE_WITHDRAW_PRECOMPILE_ADDRESS,
    WEI_PER_E8S, WRAP_PRECOMPILE_ADDRESS,
};
use crate::hash;
use evm_db::chain_data::receipt::log_entry_from_parts;
use evm_db::chain_data::RuntimeConfigV1;
use evm_db::stable_state::{init_stable_state, set_runtime_config};
use evm_db::Storable;
use proptest::prelude::*;
use revm::interpreter::{CallInput, CallInputs, CallScheme, CallValue};
use revm::primitives::{Address, Bytes, U256};
use std::borrow::Cow;

fn configure_runtime(factory: [u8; 20]) {
    init_stable_state();
    let principal = candid::Principal::self_authenticating(b"wrap-precompile-test");
    let raw = principal.as_slice();
    let mut bytes = [0u8; 64];
    bytes[0] = 1;
    bytes[1] = raw.len() as u8;
    bytes[2..2 + raw.len()].copy_from_slice(raw);
    bytes[32..52].copy_from_slice(&factory);
    set_runtime_config(RuntimeConfigV1::from_bytes(Cow::Owned(bytes.to_vec())));
}

fn encode_query_precompile_input(kind: u8, method: &str, arg: &[u8]) -> Vec<u8> {
    let target = candid::Principal::self_authenticating(b"query-target");
    encode_query_precompile_input_raw(kind, target.as_slice(), method.as_bytes(), arg)
}

fn encode_query_precompile_input_raw(
    kind: u8,
    target_bytes: &[u8],
    method_bytes: &[u8],
    arg: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(COMPACT_ICP_PRECOMPILE_FORMAT_VERSION);
    out.push(kind);
    out.push(target_bytes.len() as u8);
    out.extend_from_slice(target_bytes);
    out.push(method_bytes.len() as u8);
    out.extend_from_slice(method_bytes);
    out.extend_from_slice(&(arg.len() as u32).to_be_bytes());
    out.extend_from_slice(arg);
    out
}

#[test]
fn icp_query_precompile_compact_input_decodes() {
    let arg = vec![0x44, 0x49, 0x44, 0x4c, 0x00, 0x00];
    let input = encode_query_precompile_input(ICP_QUERY_KIND_QUERY, "read_state", &arg);
    let parsed = parse_icp_query_input(&input).expect("must decode");
    assert_eq!(parsed.method, "read_state");
    assert_eq!(parsed.arg, arg);
    assert!(!parsed.target.is_empty());
}

#[test]
fn icp_query_precompile_rejects_update_kind() {
    let input = encode_query_precompile_input(ICP_PRECOMPILE_KIND_UPDATE, "write_state", &[]);
    assert_eq!(
        parse_icp_query_input(&input).unwrap_err(),
        "ic_query.update_unimplemented"
    );
}

#[test]
fn icp_update_intent_precompile_accepts_update_kind() {
    let arg = vec![0x44, 0x49, 0x44, 0x4c];
    let input = encode_query_precompile_input(ICP_PRECOMPILE_KIND_UPDATE, "write_state", &arg);
    let parsed = parse_icp_update_intent_input(&input).expect("must decode update intent");

    assert_eq!(parsed.method, "write_state");
    assert_eq!(parsed.arg, arg);
    assert!(!parsed.target.is_empty());
}

#[test]
fn icp_update_intent_precompile_rejects_query_kind() {
    let input = encode_query_precompile_input(ICP_QUERY_KIND_QUERY, "read_state", &[]);
    assert_eq!(
        parse_icp_update_intent_input(&input).unwrap_err(),
        "ic_update.kind_invalid"
    );
}

#[test]
fn icp_query_precompile_rejects_long_method() {
    let method = "x".repeat(65);
    let input = encode_query_precompile_input(ICP_QUERY_KIND_QUERY, &method, &[]);
    assert_eq!(
        parse_icp_query_input(&input).unwrap_err(),
        "ic_query.method_invalid"
    );
}

proptest! {
    #[test]
    fn icp_query_precompile_parser_matches_verified_model_for_complete_frames(
        version in any::<u8>(),
        kind in any::<u8>(),
        target_len in 0usize..=40,
        method_len in 0usize..=80,
        method_is_utf8 in any::<bool>(),
        arg in proptest::collection::vec(any::<u8>(), 0..32),
    ) {
        let mut input = Vec::new();
        let target = vec![0x2au8; target_len];
        let method = if method_is_utf8 {
            vec![b'm'; method_len]
        } else {
            vec![0xff; method_len]
        };
        input.push(version);
        input.push(kind);
        input.push(target_len as u8);
        input.extend_from_slice(&target);
        input.push(method_len as u8);
        input.extend_from_slice(&method);
        input.extend_from_slice(&(arg.len() as u32).to_be_bytes());
        input.extend_from_slice(&arg);

        let model = verified_core::kasane_precompiles::compact_icp_query_input_safe_raw(
            u64::from(version),
            u64::from(kind),
            target_len as u64,
            1,
            method_len as u64,
            1,
            u64::from(method_is_utf8),
            1,
            1,
        );
        let parsed = parse_icp_query_input(&input);

        prop_assert_eq!(parsed.is_ok(), model);
        if model {
            let parsed = parsed.expect("model accepted frame");
            prop_assert_eq!(parsed.target, target);
            prop_assert_eq!(parsed.method.as_bytes(), method.as_slice());
            prop_assert_eq!(parsed.arg, arg);
        }
        if kind == ICP_PRECOMPILE_KIND_UPDATE {
            prop_assert!(verified_core::kasane_precompiles::icp_query_update_kind_rejected_raw(
                u64::from(kind),
            ));
        }
    }
}

#[test]
fn icp_query_parser_rejects_truncated_arg_against_verified_model() {
    let mut input = encode_query_precompile_input(ICP_QUERY_KIND_QUERY, "read_state", &[1, 2, 3]);
    input.pop();
    assert_eq!(
        parse_icp_query_input(&input).unwrap_err(),
        "ic_query.arg.abi_invalid"
    );
    assert!(
        !verified_core::kasane_precompiles::compact_icp_query_input_safe_raw(
            COMPACT_ICP_PRECOMPILE_FORMAT_VERSION as u64,
            ICP_QUERY_KIND_QUERY as u64,
            candid::Principal::self_authenticating(b"query-target")
                .as_slice()
                .len() as u64,
            1,
            "read_state".len() as u64,
            1,
            1,
            0,
            1,
        )
    );
}

#[test]
fn icp_query_parser_rejects_trailing_data_against_verified_model() {
    let mut input = encode_query_precompile_input(ICP_QUERY_KIND_QUERY, "read_state", &[1, 2, 3]);
    input.push(0);
    assert_eq!(
        parse_icp_query_input(&input).unwrap_err(),
        "ic_query.arg.abi_invalid"
    );
    assert!(
        !verified_core::kasane_precompiles::compact_icp_query_input_safe_raw(
            COMPACT_ICP_PRECOMPILE_FORMAT_VERSION as u64,
            ICP_QUERY_KIND_QUERY as u64,
            candid::Principal::self_authenticating(b"query-target")
                .as_slice()
                .len() as u64,
            1,
            "read_state".len() as u64,
            1,
            1,
            1,
            0,
        )
    );
}

#[test]
fn unwrap_intent_log_roundtrip_decodes() {
    let asset = vec![4, 5, 6];
    let amount = [8u8; 32];
    let recipient = vec![9, 10, 11];
    let mut data = Vec::new();
    data.push(asset.len() as u8);
    data.extend_from_slice(&asset);
    data.extend_from_slice(&amount);
    data.push(recipient.len() as u8);
    data.extend_from_slice(&recipient);
    let log = log_entry_from_parts(
        WRAP_PRECOMPILE_ADDRESS.into_array(),
        vec![wrap_event_topic0()],
        data,
    );
    let parsed = unwrap_intent_from_log(&log).expect("must decode");
    assert_eq!(parsed.asset_id, asset);
    assert_eq!(parsed.amount, amount);
    assert_eq!(parsed.recipient, recipient);
}

#[test]
fn native_withdraw_intent_log_roundtrip_decodes() {
    let amount = U256::from(123u64).to_be_bytes();
    let recipient = vec![9, 10, 11];
    let mut data = Vec::new();
    data.extend_from_slice(&amount);
    data.push(recipient.len() as u8);
    data.extend_from_slice(&recipient);
    let log = log_entry_from_parts(
        NATIVE_WITHDRAW_PRECOMPILE_ADDRESS.into_array(),
        vec![native_withdraw_event_topic0()],
        data,
    );
    let parsed = native_withdraw_intent_from_log(&log).expect("must decode");
    assert_eq!(parsed.amount_e8s, amount);
    assert_eq!(parsed.recipient, recipient);
}

#[test]
fn icp_update_intent_log_roundtrip_decodes() {
    let target = vec![1, 2, 3];
    let method = "write_state";
    let arg = vec![4, 5, 6];
    let mut data = Vec::new();
    data.push(target.len() as u8);
    data.extend_from_slice(&target);
    data.push(method.len() as u8);
    data.extend_from_slice(method.as_bytes());
    data.extend_from_slice(&(arg.len() as u32).to_be_bytes());
    data.extend_from_slice(&arg);
    let log = log_entry_from_parts(
        ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS.into_array(),
        vec![icp_update_intent_event_topic0()],
        data,
    );
    let parsed = icp_update_intent_from_log(&log).expect("must decode");
    assert_eq!(parsed.target, target);
    assert_eq!(parsed.method, method);
    assert_eq!(parsed.arg, arg);
}

#[test]
fn native_value_to_e8s_requires_exact_ledger_unit() {
    assert_eq!(
        native_value_to_e8s(U256::from(WEI_PER_E8S * 3)),
        Some(U256::from(3u8))
    );
    assert_eq!(native_value_to_e8s(U256::from(WEI_PER_E8S - 1)), None);
}

#[test]
fn wrap_precompile_address_points_to_reserved_high_range_slot() {
    assert_eq!(
        WRAP_PRECOMPILE_ADDRESS.into_array(),
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0x00, 0x01]
    );
}

#[test]
fn unwrap_intent_from_log_rejects_legacy_precompile_address() {
    let legacy = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1];
    let asset = vec![4, 5, 6];
    let amount = [8u8; 32];
    let recipient = vec![9, 10, 11];
    let mut data = Vec::new();
    data.push(asset.len() as u8);
    data.extend_from_slice(&asset);
    data.extend_from_slice(&amount);
    data.push(recipient.len() as u8);
    data.extend_from_slice(&recipient);
    let log = log_entry_from_parts(legacy, vec![wrap_event_topic0()], data);
    assert!(unwrap_intent_from_log(&log).is_none());
}

#[test]
fn gas_estimate_monotonic_with_input_size() {
    let small = estimate_wrap_precompile_gas(32, 64, 3);
    let large = estimate_wrap_precompile_gas(320, 64, 3);
    assert!(large > small);
}

#[test]
fn compact_decode_valid_input() {
    let encoded = encode_compact(vec![4, 5, 6], [8u8; 32], vec![9, 10, 11]);
    let parsed = parse_input(&encoded).expect("must decode");
    assert_eq!(parsed.asset_id, vec![4, 5, 6]);
    assert_eq!(parsed.amount, [8u8; 32]);
    assert_eq!(parsed.recipient, vec![9, 10, 11]);
}

#[test]
fn compact_decode_rejects_non_zero_padding() {
    let mut encoded = encode_compact(vec![4, 5, 6], [8u8; 32], vec![9, 10, 11]);
    encoded[5] = 0x7f;
    let err = parse_input(&encoded).expect_err("must reject");
    assert_eq!(err, "wrap.arg.padding_invalid");
}

#[test]
fn compact_decode_rejects_wrong_version() {
    let mut encoded = encode_compact(vec![4, 5, 6], [8u8; 32], vec![9, 10, 11]);
    encoded[0] = 2;
    let err = parse_input(&encoded).expect_err("must reject");
    assert_eq!(err, "wrap.arg.abi_invalid");
}

#[test]
fn compact_decode_rejects_trailing_data() {
    let mut encoded = encode_compact(vec![4, 5, 6], [8u8; 32], vec![9, 10, 11]);
    encoded.push(0);
    let err = parse_input(&encoded).expect_err("must reject");
    assert_eq!(err, "wrap.arg.abi_invalid");
}

#[test]
fn compact_decode_rejects_too_long_principal() {
    let mut encoded = encode_compact(vec![4, 5, 6], [8u8; 32], vec![9, 10, 11]);
    encoded[1] = 30;
    let err = parse_input(&encoded).expect_err("must reject");
    assert_eq!(err, "wrap.arg.principal_invalid");
}

#[test]
fn extra_gas_rounds_up_with_ratio() {
    assert_eq!(compute_extra_gas(0, 10, 3), 0);
    assert_eq!(compute_extra_gas(100, 0, 3), 0);
    assert_eq!(compute_extra_gas(100, 1, 3), 34);
    assert_eq!(compute_extra_gas(100, 1, 0), 100);
}

#[test]
fn extra_gas_uses_fixed_ratio() {
    assert_eq!(extra_gas_by_instruction_ratio(100), 1);
    assert_eq!(extra_gas_by_instruction_ratio(250), 3);
}

#[test]
fn unwrap_precompile_skips_instruction_ratio_extra_gas() {
    assert_eq!(
        extra_gas_for_precompile(WRAP_PRECOMPILE_ADDRESS.into_array(), 1_000),
        0
    );
}

#[test]
fn non_wrap_precompile_keeps_instruction_ratio_extra_gas() {
    let address = [0x11u8; 20];
    assert_eq!(extra_gas_for_precompile(address, 250), 3);
}

#[test]
fn compute_asset_key_matches_factory_domain_format() {
    let mut chain_bytes = [0u8; 32];
    chain_bytes[24..].copy_from_slice(&evm_db::chain_data::constants::CHAIN_ID.to_be_bytes());
    let key = compute_asset_key(&[1, 2, 3]);
    assert_eq!(
        key,
        hash::keccak256(
            &[
                b"kasane.wrap.v1".as_slice(),
                chain_bytes.as_slice(),
                &[1, 2, 3],
            ]
            .concat()
        )
    );
}

#[test]
fn allowance_slot_uses_factory_as_spender() {
    let factory = [0x33u8; 20];
    configure_runtime(factory);
    let owner = Address::new([0x11; 20]);
    let spender = Address::new(factory);
    assert_ne!(
        allowance_slot(owner, spender),
        allowance_slot(owner, Address::new([0x22; 20]))
    );
}

#[test]
fn erc20_event_topics_match_standard_signatures() {
    let owner = Address::new([0x11; 20]);
    let topic = topic_from_address(owner);
    assert_eq!(&topic.0[12..], owner.as_slice());
    assert_eq!(
        approval_event_topic0(),
        hash::keccak256(b"Approval(address,address,uint256)")
    );
    assert_eq!(
        transfer_event_topic0(),
        hash::keccak256(b"Transfer(address,address,uint256)")
    );
}

#[test]
fn unwrap_owner_uses_current_call_frame_caller() {
    let tx_origin = Address::new([0x11; 20]);
    let frame_caller = Address::new([0x22; 20]);
    let inputs = CallInputs {
        input: CallInput::Bytes(Bytes::new()),
        return_memory_offset: 0..0,
        gas_limit: 300_000,
        bytecode_address: WRAP_PRECOMPILE_ADDRESS,
        known_bytecode: None,
        target_address: WRAP_PRECOMPILE_ADDRESS,
        caller: frame_caller,
        value: CallValue::Transfer(U256::ZERO),
        scheme: CallScheme::Call,
        is_static: false,
    };

    assert_ne!(frame_caller, tx_origin);
    assert_eq!(unwrap_owner(&inputs), frame_caller);
}

fn encode_compact(asset: Vec<u8>, amount: [u8; 32], recipient: Vec<u8>) -> Vec<u8> {
    fn encode_principal(bytes: Vec<u8>) -> Vec<u8> {
        let mut out = vec![0u8; 1 + MAX_PRINCIPAL_LEN];
        out[0] = bytes.len() as u8;
        out[1..1 + bytes.len()].copy_from_slice(&bytes);
        out
    }

    let mut out = Vec::new();
    out.push(COMPACT_UNWRAP_FORMAT_VERSION);
    out.extend_from_slice(&encode_principal(asset));
    out.extend_from_slice(&amount);
    out.extend_from_slice(&encode_principal(recipient));
    out
}
