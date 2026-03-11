//! どこで: EVM custom precompile / 何を: unwrap request の起票 / なぜ: wrap/vault 連携を tx 内で確定するため

use crate::hash;
use evm_db::chain_data::receipt::LogEntry;
use revm::{
    context::Cfg,
    context_interface::{ContextTr, JournalTr, LocalContextTr},
    handler::{EthPrecompiles, PrecompileProvider},
    interpreter::{CallInputs, Gas, InstructionResult, InterpreterResult},
    primitives::{Address, Bytes, Log, B256},
};
use std::boxed::Box;
use std::cell::RefCell;
use std::collections::BTreeMap;

// 予約レンジ方針:
// - 0x00000000000000000000000000000000ffff0001: unwrap
// - 0x00000000000000000000000000000000ffff0002+: 将来拡張用の予約スロット
pub const WRAP_PRECOMPILE_ADDRESS: Address = Address::new([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0x00, 0x01,
]);
const MAX_FIELD_LEN: usize = 120;
const MAX_PRINCIPAL_LEN: usize = 29;
const COMPACT_UNWRAP_FORMAT_VERSION: u8 = 1;
const COMPACT_PRINCIPAL_FIELD_LEN: usize = 1 + MAX_PRINCIPAL_LEN;
const COMPACT_UNWRAP_INPUT_LEN: usize = 1 + COMPACT_PRINCIPAL_FIELD_LEN * 2 + 32;
const ABI_DYNAMIC_FIELDS: usize = 2;
const FIXED_PRECOMPILE_GAS_RATIO_NUMERATOR: u32 = 1;
const FIXED_PRECOMPILE_GAS_RATIO_DENOMINATOR: u32 = 100;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrecompileProfileEntry {
    pub address: [u8; 20],
    pub calls: u64,
    pub total_instructions: u128,
    pub avg_instructions: u64,
    pub max_instructions: u64,
    pub total_extra_gas: u128,
    pub avg_extra_gas: u64,
    pub max_extra_gas: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct PrecompileProfileAccumulator {
    calls: u64,
    total_instructions: u128,
    max_instructions: u64,
    total_extra_gas: u128,
    max_extra_gas: u64,
}

thread_local! {
    static PRECOMPILE_PROFILE_ACC: RefCell<BTreeMap<[u8; 20], PrecompileProfileAccumulator>> = const { RefCell::new(BTreeMap::new()) };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnwrapIntent {
    pub asset_id: Vec<u8>,
    pub amount: [u8; 32],
    pub recipient: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct WrapPrecompileProvider {
    inner: EthPrecompiles,
    allow_external: bool,
}

impl WrapPrecompileProvider {
    pub fn new(allow_external: bool) -> Self {
        Self {
            inner: EthPrecompiles::default(),
            allow_external,
        }
    }
}

impl<CTX> PrecompileProvider<CTX> for WrapPrecompileProvider
where
    CTX: ContextTr<Cfg: Cfg>,
{
    type Output = InterpreterResult;

    fn set_spec(&mut self, spec: <CTX::Cfg as Cfg>::Spec) -> bool {
        <EthPrecompiles as PrecompileProvider<CTX>>::set_spec(&mut self.inner, spec)
    }

    fn run(
        &mut self,
        context: &mut CTX,
        inputs: &CallInputs,
    ) -> Result<Option<Self::Output>, String> {
        // 課金と profile 判断は IC instruction counter を正とする。
        let started_instruction = current_instruction_counter();
        let address = inputs.bytecode_address.into_array();

        let output = if inputs.bytecode_address != WRAP_PRECOMPILE_ADDRESS {
            self.inner.run(context, inputs)?
        } else {
            Some(run_wrap_precompile(context, inputs, self.allow_external))
        };

        let Some(mut out) = output else {
            return Ok(None);
        };

        let elapsed_instruction = current_instruction_counter().saturating_sub(started_instruction);
        let extra_gas = extra_gas_for_precompile(address, elapsed_instruction);
        if extra_gas != 0 && !out.gas.record_cost(extra_gas) {
            out = InterpreterResult {
                result: InstructionResult::PrecompileOOG,
                gas: Gas::new(inputs.gas_limit),
                output: Bytes::new(),
            };
        }
        record_precompile_profile(address, elapsed_instruction, extra_gas);
        Ok(Some(out))
    }

    fn warm_addresses(&self) -> Box<impl Iterator<Item = Address>> {
        let mut addresses = vec![WRAP_PRECOMPILE_ADDRESS];
        addresses.extend(self.inner.warm_addresses());
        Box::new(addresses.into_iter())
    }

    fn contains(&self, address: &Address) -> bool {
        *address == WRAP_PRECOMPILE_ADDRESS || self.inner.contains(address)
    }
}

fn run_wrap_precompile<CTX: ContextTr>(
    context: &mut CTX,
    inputs: &CallInputs,
    allow_external: bool,
) -> InterpreterResult {
    let gas_limit = inputs.gas_limit;

    if !allow_external {
        return precompile_fail(context, gas_limit, "wrap.precompile.query_disallowed");
    }
    if inputs.is_static {
        return precompile_fail(context, gas_limit, "wrap.precompile.static_disallowed");
    }

    let input = inputs.input.bytes(context);
    let parsed = match parse_input(&input) {
        Ok(v) => v,
        Err(code) => return precompile_fail(context, gas_limit, code),
    };
    let log_data = encode_log_data(&parsed);
    let log_data_len = log_data.len();
    let log = Log::new_unchecked(
        WRAP_PRECOMPILE_ADDRESS,
        vec![B256::from(wrap_event_topic0())],
        log_data.into(),
    );
    context.journal_mut().log(log);

    let mut out = InterpreterResult {
        result: InstructionResult::Return,
        gas: Gas::new(gas_limit),
        output: Bytes::new(),
    };
    let estimated_gas = estimate_wrap_precompile_gas(input.len(), log_data_len, ABI_DYNAMIC_FIELDS);
    if !out.gas.record_cost(estimated_gas) {
        return InterpreterResult {
            result: InstructionResult::PrecompileOOG,
            gas: Gas::new(gas_limit),
            output: Bytes::new(),
        };
    }
    out
}

fn precompile_fail<CTX: ContextTr>(
    context: &mut CTX,
    gas_limit: u64,
    msg: &str,
) -> InterpreterResult {
    context
        .local_mut()
        .set_precompile_error_context(msg.to_string());
    InterpreterResult {
        result: InstructionResult::PrecompileError,
        gas: Gas::new(gas_limit),
        output: Bytes::new(),
    }
}

fn parse_input(input: &[u8]) -> Result<UnwrapIntent, &'static str> {
    parse_compact_input(input)
}

fn parse_compact_input(input: &[u8]) -> Result<UnwrapIntent, &'static str> {
    if input.len() != COMPACT_UNWRAP_INPUT_LEN {
        return Err("wrap.arg.abi_invalid");
    }
    if input[0] != COMPACT_UNWRAP_FORMAT_VERSION {
        return Err("wrap.arg.abi_invalid");
    }
    let mut offset = 1usize;
    let asset_id = read_compact_principal(input, &mut offset)?;
    let amount = read_array_32(input, &mut offset).ok_or("wrap.arg.amount_invalid")?;
    let recipient = read_compact_principal(input, &mut offset)?;
    if offset != input.len() {
        return Err("wrap.arg.abi_invalid");
    }
    Ok(UnwrapIntent {
        asset_id,
        amount,
        recipient,
    })
}

pub(crate) fn estimate_wrap_precompile_gas(
    input_len: usize,
    log_data_len: usize,
    field_count: usize,
) -> u64 {
    let base_gas = 25_000u64;
    let per_byte_gas = 16u64.saturating_mul(input_len as u64);
    let per_field_gas = 200u64.saturating_mul(field_count as u64);
    let topic_count = 1u64;
    let log_gas = 375u64
        .saturating_add(375u64.saturating_mul(topic_count))
        .saturating_add(8u64.saturating_mul(log_data_len as u64));
    base_gas
        .saturating_add(per_byte_gas)
        .saturating_add(per_field_gas)
        .saturating_add(log_gas)
}

fn encode_log_data(intent: &UnwrapIntent) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + 32 + 2);
    out.push(intent.asset_id.len() as u8);
    out.extend_from_slice(&intent.asset_id);
    out.extend_from_slice(&intent.amount);
    out.push(intent.recipient.len() as u8);
    out.extend_from_slice(&intent.recipient);
    out
}

pub fn unwrap_intent_from_log(log: &LogEntry) -> Option<UnwrapIntent> {
    if log.address.into_array() != WRAP_PRECOMPILE_ADDRESS.into_array() {
        return None;
    }
    let topics = log.topics();
    if topics.len() != 1 || topics[0].0 != wrap_event_topic0() {
        return None;
    }
    let data = log.data.data.as_ref();
    let mut offset = 0usize;
    let asset_id = read_len_prefixed(data, &mut offset)?;
    let amount = read_array_32(data, &mut offset)?;
    let recipient = read_len_prefixed(data, &mut offset)?;
    if offset != data.len() {
        return None;
    }
    Some(UnwrapIntent {
        asset_id,
        amount,
        recipient,
    })
}

fn wrap_event_topic0() -> [u8; 32] {
    hash::keccak256(b"KasaneUnwrapRequest(bytes)")
}

fn is_valid_principal_bytes(len: usize) -> bool {
    (1..=MAX_PRINCIPAL_LEN).contains(&len)
}

fn read_compact_principal(input: &[u8], offset: &mut usize) -> Result<Vec<u8>, &'static str> {
    let len = *input.get(*offset).ok_or("wrap.arg.abi_invalid")? as usize;
    *offset = offset.saturating_add(1);
    if !is_valid_principal_bytes(len) {
        return Err("wrap.arg.principal_invalid");
    }
    let end = offset
        .checked_add(MAX_PRINCIPAL_LEN)
        .ok_or("wrap.arg.abi_invalid")?;
    let slot = input.get(*offset..end).ok_or("wrap.arg.abi_invalid")?;
    if slot[len..].iter().any(|&byte| byte != 0) {
        return Err("wrap.arg.padding_invalid");
    }
    let bytes = slot[..len].to_vec();
    *offset = end;
    Ok(bytes)
}

fn read_len_prefixed(data: &[u8], offset: &mut usize) -> Option<Vec<u8>> {
    let len = *data.get(*offset)? as usize;
    *offset = offset.saturating_add(1);
    if len == 0 || len > MAX_FIELD_LEN {
        return None;
    }
    let end = offset.checked_add(len)?;
    let bytes = data.get(*offset..end)?.to_vec();
    *offset = end;
    Some(bytes)
}

fn read_array_32(data: &[u8], offset: &mut usize) -> Option<[u8; 32]> {
    let end = offset.checked_add(32)?;
    let slice = data.get(*offset..end)?;
    let mut out = [0u8; 32];
    out.copy_from_slice(slice);
    *offset = end;
    Some(out)
}

fn current_instruction_counter() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        return ic_cdk::api::performance_counter(
            ic_cdk::api::PerformanceCounterType::InstructionCounter,
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0
    }
}

fn extra_gas_by_instruction_ratio(elapsed_instruction: u64) -> u64 {
    compute_extra_gas(
        elapsed_instruction,
        FIXED_PRECOMPILE_GAS_RATIO_NUMERATOR,
        FIXED_PRECOMPILE_GAS_RATIO_DENOMINATOR,
    )
}

fn extra_gas_for_precompile(address: [u8; 20], elapsed_instruction: u64) -> u64 {
    if address == WRAP_PRECOMPILE_ADDRESS.into_array() {
        return 0;
    }
    extra_gas_by_instruction_ratio(elapsed_instruction)
}

fn compute_extra_gas(elapsed_instruction: u64, numerator: u32, denominator: u32) -> u64 {
    if elapsed_instruction == 0 || numerator == 0 {
        return 0;
    }
    let denominator = denominator.max(1);
    let scaled = u128::from(elapsed_instruction).saturating_mul(u128::from(numerator));
    let rounded =
        scaled.saturating_add(u128::from(denominator).saturating_sub(1)) / u128::from(denominator);
    rounded.min(u128::from(u64::MAX)) as u64
}

fn record_precompile_profile(address: [u8; 20], elapsed_instruction: u64, extra_gas: u64) {
    PRECOMPILE_PROFILE_ACC.with(|map| {
        let mut map = map.borrow_mut();
        let entry = map
            .entry(address)
            .or_insert_with(PrecompileProfileAccumulator::default);
        entry.calls = entry.calls.saturating_add(1);
        entry.total_instructions = entry
            .total_instructions
            .saturating_add(u128::from(elapsed_instruction));
        entry.max_instructions = entry.max_instructions.max(elapsed_instruction);
        entry.total_extra_gas = entry.total_extra_gas.saturating_add(u128::from(extra_gas));
        entry.max_extra_gas = entry.max_extra_gas.max(extra_gas);
    });
}

pub fn precompile_profile_snapshot() -> Vec<PrecompileProfileEntry> {
    PRECOMPILE_PROFILE_ACC.with(|map| {
        map.borrow()
            .iter()
            .map(|(address, acc)| {
                let calls = acc.calls.max(1);
                PrecompileProfileEntry {
                    address: *address,
                    calls: acc.calls,
                    total_instructions: acc.total_instructions,
                    avg_instructions: (acc.total_instructions / u128::from(calls)) as u64,
                    max_instructions: acc.max_instructions,
                    total_extra_gas: acc.total_extra_gas,
                    avg_extra_gas: (acc.total_extra_gas / u128::from(calls)) as u64,
                    max_extra_gas: acc.max_extra_gas,
                }
            })
            .collect()
    })
}

pub fn clear_precompile_profile() {
    PRECOMPILE_PROFILE_ACC.with(|map| map.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use super::{
        compute_extra_gas, estimate_wrap_precompile_gas, extra_gas_by_instruction_ratio,
        extra_gas_for_precompile, parse_input, unwrap_intent_from_log, wrap_event_topic0,
        COMPACT_UNWRAP_FORMAT_VERSION, MAX_PRINCIPAL_LEN, WRAP_PRECOMPILE_ADDRESS,
    };
    use evm_db::chain_data::receipt::log_entry_from_parts;

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
}
