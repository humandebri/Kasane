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

pub const WRAP_PRECOMPILE_ADDRESS: Address =
    Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1]);
const MAX_FIELD_LEN: usize = 120;
const MAX_PRINCIPAL_LEN: usize = 29;
const ABI_WORD_SIZE: usize = 32;
const ABI_HEAD_WORDS: usize = 6;
const ABI_DYNAMIC_FIELDS: usize = 3;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnwrapIntent {
    pub request_id: [u8; 32],
    pub vault_canister_id: Vec<u8>,
    pub asset_id: Vec<u8>,
    pub amount: [u8; 32],
    pub recipient: Vec<u8>,
    pub user_nonce: u64,
    pub deadline: u64,
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
        if inputs.bytecode_address != WRAP_PRECOMPILE_ADDRESS {
            return self.inner.run(context, inputs);
        }
        Ok(Some(run_wrap_precompile(
            context,
            inputs,
            self.allow_external,
        )))
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

    let input = inputs.input.bytes(context).to_vec();
    let parsed = match parse_input(&input) {
        Ok(v) => v,
        Err(code) => return precompile_fail(context, gas_limit, code),
    };
    let mut hash_input = Vec::with_capacity(20 + input.len());
    hash_input.extend_from_slice(inputs.caller.as_slice());
    hash_input.extend_from_slice(&input);
    let request_id = hash::keccak256(&hash_input);

    let log_data = encode_log_data(&parsed, &request_id);
    let log_data_len = log_data.len();
    let topic1 = B256::from(request_id);
    let log = Log::new_unchecked(
        WRAP_PRECOMPILE_ADDRESS,
        vec![B256::from(wrap_event_topic0()), topic1],
        log_data.into(),
    );
    context.journal_mut().log(log);

    let mut out = InterpreterResult {
        result: InstructionResult::Return,
        gas: Gas::new(gas_limit),
        output: Bytes::from(request_id.to_vec()),
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
    if input.len() < ABI_WORD_SIZE * ABI_HEAD_WORDS || !input.len().is_multiple_of(ABI_WORD_SIZE) {
        return Err("wrap.arg.abi_invalid");
    }
    let vault_offset = decode_offset(
        read_word(input, 0).ok_or("wrap.arg.abi_invalid")?,
        input.len(),
    )?;
    let asset_offset = decode_offset(
        read_word(input, 1).ok_or("wrap.arg.abi_invalid")?,
        input.len(),
    )?;
    let amount = read_word(input, 2).ok_or("wrap.arg.amount_invalid")?;
    let recipient_offset = decode_offset(
        read_word(input, 3).ok_or("wrap.arg.abi_invalid")?,
        input.len(),
    )?;
    let user_nonce = decode_u64(read_word(input, 4).ok_or("wrap.arg.nonce_invalid")?)?;
    let deadline = decode_u64(read_word(input, 5).ok_or("wrap.arg.deadline_invalid")?)?;

    if !(vault_offset < asset_offset && asset_offset < recipient_offset) {
        return Err("wrap.arg.offset_invalid");
    }
    let (vault_canister_id, vault_end) =
        read_dynamic_bytes(input, vault_offset, MAX_FIELD_LEN).ok_or("wrap.arg.offset_invalid")?;
    let (asset_id, asset_end) =
        read_dynamic_bytes(input, asset_offset, MAX_FIELD_LEN).ok_or("wrap.arg.offset_invalid")?;
    let (recipient, recipient_end) = read_dynamic_bytes(input, recipient_offset, MAX_FIELD_LEN)
        .ok_or("wrap.arg.offset_invalid")?;

    if vault_end != asset_offset || asset_end != recipient_offset {
        return Err("wrap.arg.offset_invalid");
    }
    if recipient_end != input.len() {
        return Err("wrap.arg.abi_invalid");
    }

    if vault_canister_id.is_empty() || asset_id.is_empty() || recipient.is_empty() {
        return Err("wrap.arg.length_invalid");
    }
    if !is_valid_principal_bytes(vault_canister_id.len())
        || !is_valid_principal_bytes(asset_id.len())
        || !is_valid_principal_bytes(recipient.len())
    {
        return Err("wrap.arg.principal_invalid");
    }

    Ok(UnwrapIntent {
        request_id: [0u8; 32],
        vault_canister_id,
        asset_id,
        amount,
        recipient,
        user_nonce,
        deadline,
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
    let topic_count = 2u64;
    let log_gas = 375u64
        .saturating_add(375u64.saturating_mul(topic_count))
        .saturating_add(8u64.saturating_mul(log_data_len as u64));
    base_gas
        .saturating_add(per_byte_gas)
        .saturating_add(per_field_gas)
        .saturating_add(log_gas)
}

fn encode_log_data(intent: &UnwrapIntent, request_id: &[u8; 32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + 2 + 32 + 2 + 16);
    out.extend_from_slice(request_id);
    out.push(intent.vault_canister_id.len() as u8);
    out.extend_from_slice(&intent.vault_canister_id);
    out.push(intent.asset_id.len() as u8);
    out.extend_from_slice(&intent.asset_id);
    out.extend_from_slice(&intent.amount);
    out.push(intent.recipient.len() as u8);
    out.extend_from_slice(&intent.recipient);
    out.extend_from_slice(&intent.user_nonce.to_be_bytes());
    out.extend_from_slice(&intent.deadline.to_be_bytes());
    out
}

pub fn unwrap_intent_from_log(log: &LogEntry) -> Option<UnwrapIntent> {
    if (log.address.0).0 != WRAP_PRECOMPILE_ADDRESS.into_array() {
        return None;
    }
    let topics = log.topics();
    if topics.len() < 2 || topics[0].0 != wrap_event_topic0() {
        return None;
    }
    let data = log.data.data.as_ref();
    let mut offset = 0usize;
    let request_id = read_array_32(data, &mut offset)?;
    let vault_canister_id = read_len_prefixed(data, &mut offset)?;
    let asset_id = read_len_prefixed(data, &mut offset)?;
    let amount = read_array_32(data, &mut offset)?;
    let recipient = read_len_prefixed(data, &mut offset)?;
    let user_nonce = read_u64(data, &mut offset)?;
    let deadline = read_u64(data, &mut offset)?;
    if offset != data.len() || request_id != topics[1].0 {
        return None;
    }
    Some(UnwrapIntent {
        request_id,
        vault_canister_id,
        asset_id,
        amount,
        recipient,
        user_nonce,
        deadline,
    })
}

fn wrap_event_topic0() -> [u8; 32] {
    hash::keccak256(b"KasaneUnwrapRequest(bytes32,bytes)")
}

fn read_word(input: &[u8], word_index: usize) -> Option<[u8; 32]> {
    let start = word_index.checked_mul(ABI_WORD_SIZE)?;
    let end = start.checked_add(ABI_WORD_SIZE)?;
    let raw = input.get(start..end)?;
    let mut out = [0u8; 32];
    out.copy_from_slice(raw);
    Some(out)
}

fn decode_offset(word: [u8; 32], input_len: usize) -> Result<usize, &'static str> {
    if word[0..24] != [0u8; 24] {
        return Err("wrap.arg.offset_invalid");
    }
    let mut low = [0u8; 8];
    low.copy_from_slice(&word[24..32]);
    let raw_offset = u64::from_be_bytes(low);
    let offset = usize::try_from(raw_offset).map_err(|_| "wrap.arg.offset_invalid")?;
    if offset % ABI_WORD_SIZE != 0 {
        return Err("wrap.arg.offset_invalid");
    }
    if offset < ABI_WORD_SIZE * ABI_HEAD_WORDS {
        return Err("wrap.arg.offset_invalid");
    }
    if offset > input_len.saturating_sub(ABI_WORD_SIZE) {
        return Err("wrap.arg.offset_invalid");
    }
    Ok(offset)
}

fn decode_u64(word: [u8; 32]) -> Result<u64, &'static str> {
    if word[0..24] != [0u8; 24] {
        return Err("wrap.arg.length_invalid");
    }
    let mut out = [0u8; 8];
    out.copy_from_slice(&word[24..32]);
    Ok(u64::from_be_bytes(out))
}

fn read_dynamic_bytes(input: &[u8], offset: usize, max_len: usize) -> Option<(Vec<u8>, usize)> {
    let len_end = offset.checked_add(ABI_WORD_SIZE)?;
    let len_word = input.get(offset..len_end)?;
    if len_word[0..24] != [0u8; 24] {
        return None;
    }
    let mut low = [0u8; 8];
    low.copy_from_slice(&len_word[24..32]);
    let len = u64::from_be_bytes(low) as usize;
    if len == 0 || len > max_len {
        return None;
    }
    let data_start = offset.checked_add(ABI_WORD_SIZE)?;
    let data_end = data_start.checked_add(len)?;
    let padded_end = data_start.checked_add(len.checked_add(31)? / 32 * 32)?;
    if padded_end > input.len() {
        return None;
    }
    let bytes = input.get(data_start..data_end)?.to_vec();
    Some((bytes, padded_end))
}

fn is_valid_principal_bytes(len: usize) -> bool {
    (1..=MAX_PRINCIPAL_LEN).contains(&len)
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

fn read_u64(data: &[u8], offset: &mut usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    let slice = data.get(*offset..end)?;
    let mut out = [0u8; 8];
    out.copy_from_slice(slice);
    *offset = end;
    Some(u64::from_be_bytes(out))
}

#[cfg(test)]
mod tests {
    use super::{
        estimate_wrap_precompile_gas, parse_input, unwrap_intent_from_log, wrap_event_topic0,
        WRAP_PRECOMPILE_ADDRESS,
    };
    use evm_db::chain_data::receipt::log_entry_from_parts;

    #[test]
    fn unwrap_intent_log_roundtrip_decodes() {
        let request_id = [7u8; 32];
        let vault = vec![1, 2, 3];
        let asset = vec![4, 5, 6];
        let amount = [8u8; 32];
        let recipient = vec![9, 10, 11];
        let nonce = 12u64;
        let deadline = 34u64;
        let mut data = Vec::new();
        data.extend_from_slice(&request_id);
        data.push(vault.len() as u8);
        data.extend_from_slice(&vault);
        data.push(asset.len() as u8);
        data.extend_from_slice(&asset);
        data.extend_from_slice(&amount);
        data.push(recipient.len() as u8);
        data.extend_from_slice(&recipient);
        data.extend_from_slice(&nonce.to_be_bytes());
        data.extend_from_slice(&deadline.to_be_bytes());
        let log = log_entry_from_parts(
            WRAP_PRECOMPILE_ADDRESS.into_array(),
            vec![wrap_event_topic0(), request_id],
            data,
        );
        let parsed = unwrap_intent_from_log(&log).expect("must decode");
        assert_eq!(parsed.request_id, request_id);
        assert_eq!(parsed.vault_canister_id, vault);
        assert_eq!(parsed.asset_id, asset);
        assert_eq!(parsed.amount, amount);
        assert_eq!(parsed.recipient, recipient);
        assert_eq!(parsed.user_nonce, nonce);
        assert_eq!(parsed.deadline, deadline);
    }

    #[test]
    fn gas_estimate_monotonic_with_input_size() {
        let small = estimate_wrap_precompile_gas(32, 64, 3);
        let large = estimate_wrap_precompile_gas(320, 64, 3);
        assert!(large > small);
    }

    #[test]
    fn abi_decode_valid_input() {
        let encoded = encode_abi(
            vec![1, 2, 3],
            vec![4, 5, 6],
            [8u8; 32],
            vec![9, 10, 11],
            12,
            34,
        );
        let parsed = parse_input(&encoded).expect("must decode");
        assert_eq!(parsed.vault_canister_id, vec![1, 2, 3]);
        assert_eq!(parsed.asset_id, vec![4, 5, 6]);
        assert_eq!(parsed.amount, [8u8; 32]);
        assert_eq!(parsed.recipient, vec![9, 10, 11]);
        assert_eq!(parsed.user_nonce, 12);
        assert_eq!(parsed.deadline, 34);
    }

    #[test]
    fn abi_decode_rejects_invalid_offset() {
        let mut encoded = encode_abi(
            vec![1, 2, 3],
            vec![4, 5, 6],
            [8u8; 32],
            vec![9, 10, 11],
            12,
            34,
        );
        encoded[31] = 1; // first offset must be >= head size
        let err = parse_input(&encoded).expect_err("must reject");
        assert_eq!(err, "wrap.arg.offset_invalid");
    }

    #[test]
    fn abi_decode_rejects_trailing_data() {
        let mut encoded = encode_abi(
            vec![1, 2, 3],
            vec![4, 5, 6],
            [8u8; 32],
            vec![9, 10, 11],
            12,
            34,
        );
        encoded.extend_from_slice(&[0u8; 32]);
        let err = parse_input(&encoded).expect_err("must reject");
        assert_eq!(err, "wrap.arg.abi_invalid");
    }

    #[test]
    fn abi_decode_rejects_descending_offsets() {
        let mut encoded = encode_abi(
            vec![1, 2, 3],
            vec![4, 5, 6],
            [8u8; 32],
            vec![9, 10, 11],
            12,
            34,
        );
        let vault_offset = 32u64 * 6;
        encoded[56..64].copy_from_slice(&vault_offset.to_be_bytes());
        let err = parse_input(&encoded).expect_err("must reject");
        assert_eq!(err, "wrap.arg.offset_invalid");
    }

    #[test]
    fn abi_decode_rejects_too_long_principal() {
        let encoded = encode_abi(
            vec![1u8; 30],
            vec![4, 5, 6],
            [8u8; 32],
            vec![9, 10, 11],
            12,
            34,
        );
        let err = parse_input(&encoded).expect_err("must reject");
        assert_eq!(err, "wrap.arg.principal_invalid");
    }

    fn encode_abi(
        vault: Vec<u8>,
        asset: Vec<u8>,
        amount: [u8; 32],
        recipient: Vec<u8>,
        nonce: u64,
        deadline: u64,
    ) -> Vec<u8> {
        let mut out = vec![0u8; 32 * 6];
        let vault_tail = encode_dynamic(vault);
        let asset_tail = encode_dynamic(asset);
        let recipient_tail = encode_dynamic(recipient);
        let vault_offset = 32 * 6;
        let asset_offset = vault_offset + vault_tail.len();
        let recipient_offset = asset_offset + asset_tail.len();
        out[24..32].copy_from_slice(&(vault_offset as u64).to_be_bytes());
        out[56..64].copy_from_slice(&(asset_offset as u64).to_be_bytes());
        out[64..96].copy_from_slice(&amount);
        out[120..128].copy_from_slice(&(recipient_offset as u64).to_be_bytes());
        out[152..160].copy_from_slice(&nonce.to_be_bytes());
        out[184..192].copy_from_slice(&deadline.to_be_bytes());
        out.extend_from_slice(&vault_tail);
        out.extend_from_slice(&asset_tail);
        out.extend_from_slice(&recipient_tail);
        out
    }

    fn encode_dynamic(bytes: Vec<u8>) -> Vec<u8> {
        let mut out = vec![0u8; 32];
        out[24..32].copy_from_slice(&(bytes.len() as u64).to_be_bytes());
        out.extend_from_slice(&bytes);
        let pad = (32 - (bytes.len() % 32)) % 32;
        out.extend(std::iter::repeat_n(0u8, pad));
        out
    }
}
