//! どこで: Phase1のTxデコード / 何を: IcSyntheticの最小フォーマット / なぜ: 決定的な解釈のため

use alloy_rlp::{Bytes as RlpBytes, Rlp};
use evm_db::chain_data::constants::{CHAIN_ID, MAX_TX_SIZE};
use evm_db::chain_data::TxKind;
use revm::context::TxEnv;
use revm::precompile::secp256k1::ec_recover_run;
use revm::primitives::{Address, Bytes, TxKind as RevmTxKind, U256};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecodeError {
    InvalidLength,
    InvalidVersion,
    DataTooLarge,
    UnsupportedType,
    InvalidSignature,
}

// IcSynthetic v1: [version:1][to:20][value:32][gas_limit:8][nonce:8][data_len:4][data]
const IC_TX_VERSION: u8 = 1;
const IC_TX_HEADER_LEN: usize = 1 + 20 + 32 + 8 + 8 + 4;

pub fn decode_ic_synthetic(caller: Address, bytes: &[u8]) -> Result<TxEnv, DecodeError> {
    if bytes.len() < IC_TX_HEADER_LEN {
        return Err(DecodeError::InvalidLength);
    }
    if bytes[0] != IC_TX_VERSION {
        return Err(DecodeError::InvalidVersion);
    }
    let mut offset = 1;
    let mut to = [0u8; 20];
    to.copy_from_slice(&bytes[offset..offset + 20]);
    offset += 20;
    let mut value = [0u8; 32];
    value.copy_from_slice(&bytes[offset..offset + 32]);
    offset += 32;
    let mut gas = [0u8; 8];
    gas.copy_from_slice(&bytes[offset..offset + 8]);
    offset += 8;
    let mut nonce = [0u8; 8];
    nonce.copy_from_slice(&bytes[offset..offset + 8]);
    offset += 8;
    let mut len = [0u8; 4];
    len.copy_from_slice(&bytes[offset..offset + 4]);
    offset += 4;
    let data_len =
        usize::try_from(u32::from_be_bytes(len)).map_err(|_| DecodeError::InvalidLength)?;
    let expected = IC_TX_HEADER_LEN + data_len;
    if expected != bytes.len() {
        return Err(DecodeError::InvalidLength);
    }
    if data_len > MAX_TX_SIZE {
        return Err(DecodeError::DataTooLarge);
    }
    let data = bytes[offset..].to_vec();
    let tx = TxEnv {
        caller,
        gas_limit: u64::from_be_bytes(gas),
        gas_price: 0,
        kind: RevmTxKind::Call(Address::from(to)),
        value: U256::from_be_bytes(value),
        data: Bytes::from(data),
        nonce: u64::from_be_bytes(nonce),
        chain_id: Some(CHAIN_ID),
        access_list: Default::default(),
        gas_priority_fee: None,
        blob_hashes: Default::default(),
        max_fee_per_blob_gas: 0,
        authorization_list: Default::default(),
        tx_type: 0,
    };
    Ok(tx)
}

pub fn decode_tx(kind: TxKind, caller: Address, bytes: &[u8]) -> Result<TxEnv, DecodeError> {
    match kind {
        TxKind::IcSynthetic => decode_ic_synthetic(caller, bytes),
        TxKind::EthSigned => decode_eth_raw_tx(bytes),
    }
}

pub fn decode_eth_raw_tx(bytes: &[u8]) -> Result<TxEnv, DecodeError> {
    if bytes.is_empty() {
        return Err(DecodeError::InvalidLength);
    }
    // EIP-2718 typed tx: first byte < 0x80 and not a list prefix
    if bytes[0] <= 0x7f && bytes[0] != 0xc0 && bytes[0] != 0xf8 {
        return Err(DecodeError::UnsupportedType);
    }
    let mut rlp = Rlp::new(bytes).map_err(|_| DecodeError::InvalidLength)?;
    let mut fields: Vec<RlpBytes> = Vec::with_capacity(9);
    while let Some(item) = rlp.get_next::<RlpBytes>().map_err(|_| DecodeError::InvalidLength)? {
        fields.push(item);
    }
    if fields.len() != 9 {
        return Err(DecodeError::InvalidLength);
    }

    let nonce = parse_u64(&fields[0])?;
    let gas_price = parse_u128(&fields[1])?;
    let gas_limit = parse_u64(&fields[2])?;
    let to = fields[3].clone();
    let value = parse_u256(&fields[4])?;
    let data = fields[5].clone();
    let v = parse_u64(&fields[6])?;
    let r = parse_bytes_32(&fields[7])?;
    let s = parse_bytes_32(&fields[8])?;

    let (chain_id, recid) = v_to_chain_id(v)?;
    let msg = legacy_signing_hash(&fields, chain_id)?;
    let caller = recover_legacy_sender(msg, r, s, recid)?;

    let kind = if to.is_empty() {
        RevmTxKind::Create
    } else {
        let mut addr = [0u8; 20];
        if to.len() != 20 {
            return Err(DecodeError::InvalidLength);
        }
        addr.copy_from_slice(&to);
        RevmTxKind::Call(Address::from(addr))
    };

    Ok(TxEnv {
        caller,
        gas_limit,
        gas_price,
        kind,
        value,
        data: Bytes::from(data.to_vec()),
        nonce,
        chain_id,
        access_list: Default::default(),
        gas_priority_fee: Some(gas_price),
        blob_hashes: Default::default(),
        max_fee_per_blob_gas: 0,
        authorization_list: Default::default(),
        tx_type: 0,
    })
}

fn parse_u64(bytes: &RlpBytes) -> Result<u64, DecodeError> {
    if bytes.len() > 8 {
        return Err(DecodeError::InvalidLength);
    }
    let mut buf = [0u8; 8];
    buf[8 - bytes.len()..].copy_from_slice(bytes);
    Ok(u64::from_be_bytes(buf))
}

fn parse_u128(bytes: &RlpBytes) -> Result<u128, DecodeError> {
    if bytes.len() > 16 {
        return Err(DecodeError::InvalidLength);
    }
    let mut buf = [0u8; 16];
    buf[16 - bytes.len()..].copy_from_slice(bytes);
    Ok(u128::from_be_bytes(buf))
}

fn parse_u256(bytes: &RlpBytes) -> Result<U256, DecodeError> {
    if bytes.len() > 32 {
        return Err(DecodeError::InvalidLength);
    }
    let mut buf = [0u8; 32];
    buf[32 - bytes.len()..].copy_from_slice(bytes);
    Ok(U256::from_be_bytes(buf))
}

fn parse_bytes_32(bytes: &RlpBytes) -> Result<[u8; 32], DecodeError> {
    if bytes.len() > 32 {
        return Err(DecodeError::InvalidLength);
    }
    let mut buf = [0u8; 32];
    buf[32 - bytes.len()..].copy_from_slice(bytes);
    Ok(buf)
}

fn v_to_chain_id(v: u64) -> Result<(Option<u64>, u8), DecodeError> {
    if v == 27 || v == 28 {
        let recid = u8::try_from(v - 27).map_err(|_| DecodeError::InvalidSignature)?;
        return Ok((None, recid));
    }
    if v >= 35 {
        let chain_id = (v - 35) / 2;
        let recid = u8::try_from((v - 35) % 2).map_err(|_| DecodeError::InvalidSignature)?;
        return Ok((Some(chain_id), recid));
    }
    Err(DecodeError::InvalidSignature)
}

fn legacy_signing_hash(fields: &[RlpBytes], chain_id: Option<u64>) -> Result<[u8; 32], DecodeError> {
    let mut list: Vec<RlpBytes> = Vec::with_capacity(9);
    for item in fields.iter().take(6) {
        list.push(trim_rlp_bytes(item));
    }
    if let Some(id) = chain_id {
        list.push(int_to_rlp_bytes(id));
        list.push(RlpBytes::new());
        list.push(RlpBytes::new());
    }
    let mut out = Vec::new();
    alloy_rlp::encode_list::<RlpBytes, [u8]>(&list, &mut out);
    Ok(crate::hash::keccak256(&out))
}

fn int_to_rlp_bytes(value: u64) -> RlpBytes {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&value.to_be_bytes());
    let trimmed = trim_bytes(&buf);
    RlpBytes::from(trimmed.to_vec())
}

fn trim_bytes(bytes: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < bytes.len() && bytes[i] == 0 {
        i += 1;
    }
    &bytes[i..]
}

fn trim_rlp_bytes(bytes: &RlpBytes) -> RlpBytes {
    let trimmed = trim_bytes(bytes);
    RlpBytes::from(trimmed.to_vec())
}

fn recover_legacy_sender(
    msg: [u8; 32],
    r: [u8; 32],
    s: [u8; 32],
    recid: u8,
) -> Result<Address, DecodeError> {
    let v_byte = 27u8
        .checked_add(recid)
        .ok_or(DecodeError::InvalidSignature)?;
    // ecrecover precompile input: msg || v(32) || r(32) || s(32)
    let mut input = [0u8; 128];
    input[0..32].copy_from_slice(&msg);
    input[63] = v_byte;
    input[64..96].copy_from_slice(&r);
    input[96..128].copy_from_slice(&s);
    let output = ec_recover_run(&input, u64::MAX).map_err(|_| DecodeError::InvalidSignature)?;
    if output.bytes.len() != 32 {
        return Err(DecodeError::InvalidSignature);
    }
    Ok(Address::from_slice(&output.bytes[12..32]))
}
