//! どこで: Phase1のTxデコード / 何を: IcSynthetic + Eth の安全なデコード / なぜ: 互換性とtrap回避
use alloy_primitives::Address as AlloyAddress;
use byteorder::{BigEndian, ByteOrder};
use evm_db::chain_data::constants::{CHAIN_ID, MAX_TX_SIZE};
use evm_db::chain_data::TxKind;
use evm_tx::{recover_eth_tx, RecoveredTx, RecoveryError};
use revm::context::TxEnv;
use revm::context_interface::transaction::{AccessList, AccessListItem};
use revm::primitives::{
    Address as RevmAddress, Bytes as RevmBytes, TxKind as RevmTxKind, B256, U256 as RevmU256,
};
use std::borrow::Cow;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecodeError {
    InvalidLength,
    InvalidVersion,
    DataTooLarge,
    UnsupportedType,
    LegacyChainIdMissing,
    WrongChainId,
    InvalidSignature,
    InvalidRlp,
    TrailingBytes,
}

// IcSynthetic canonical bytes:
// [to_flag:1][to?:20][value:32][gas_limit:8][nonce:8]
// [max_fee_per_gas:16][max_priority_fee_per_gas:16][data_len:4][data]
const IC_TX_TO_FLAG_NONE: u8 = 0;
const IC_TX_TO_FLAG_SOME: u8 = 1;
const IC_TX_BASE_HEADER_LEN: usize = 1 + 32 + 8 + 8 + 16 + 16 + 4;
const IC_TX_TO_LEN: usize = 20;
const TX_TYPE_EIP4844: u8 = 0x03;
const TX_TYPE_EIP7702: u8 = 0x04;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IcSyntheticTxInput {
    pub to: Option<[u8; 20]>,
    pub value: [u8; 32],
    pub gas_limit: u64,
    pub nonce: u64,
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IcTxHeader<'a> {
    pub to: Option<[u8; 20]>,
    pub value: [u8; 32],
    pub gas_limit: u64,
    pub nonce: u64,
    pub max_fee: u128,
    pub max_priority: u128,
    pub data: &'a [u8],
}

pub fn decode_ic_synthetic_header(bytes: &[u8]) -> Result<IcTxHeader<'_>, DecodeError> {
    decode_ic_synthetic_header_impl::<true>(bytes)
}

fn decode_ic_synthetic_header_impl<const ENFORCE_DATA_SIZE_LIMIT: bool>(
    bytes: &[u8],
) -> Result<IcTxHeader<'_>, DecodeError> {
    if bytes.len() < IC_TX_BASE_HEADER_LEN {
        return Err(DecodeError::InvalidLength);
    }
    let to_len = match bytes.first().copied() {
        Some(IC_TX_TO_FLAG_NONE) => 0usize,
        Some(IC_TX_TO_FLAG_SOME) => IC_TX_TO_LEN,
        Some(_) => return Err(DecodeError::InvalidVersion),
        None => unreachable!("base header length check guarantees first byte"),
    };
    let fixed_len = IC_TX_BASE_HEADER_LEN + to_len;
    if bytes.len() < fixed_len {
        return Err(DecodeError::InvalidLength);
    }
    let mut offset = 1usize;
    let to = if to_len == 0 {
        None
    } else {
        let mut to = [0u8; IC_TX_TO_LEN];
        to.copy_from_slice(&bytes[offset..offset + IC_TX_TO_LEN]);
        offset += IC_TX_TO_LEN;
        Some(to)
    };
    let mut value = [0u8; 32];
    value.copy_from_slice(&bytes[offset..offset + 32]);
    offset += 32;
    let gas_limit = BigEndian::read_u64(&bytes[offset..offset + 8]);
    offset += 8;
    let nonce = BigEndian::read_u64(&bytes[offset..offset + 8]);
    offset += 8;
    let max_fee = BigEndian::read_u128(&bytes[offset..offset + 16]);
    offset += 16;
    let max_priority = BigEndian::read_u128(&bytes[offset..offset + 16]);
    offset += 16;
    let data_len = BigEndian::read_u32(&bytes[offset..offset + 4]) as usize;
    offset += 4;
    if bytes.len() - offset != data_len {
        return Err(DecodeError::InvalidLength);
    }
    if ENFORCE_DATA_SIZE_LIMIT && data_len > MAX_TX_SIZE {
        return Err(DecodeError::DataTooLarge);
    }
    Ok(IcTxHeader {
        to,
        value,
        gas_limit,
        nonce,
        max_fee,
        max_priority,
        data: &bytes[offset..],
    })
}

pub fn encode_ic_synthetic_input(input: &IcSyntheticTxInput) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        IC_TX_BASE_HEADER_LEN
            + input.data.len()
            + if input.to.is_some() { IC_TX_TO_LEN } else { 0 },
    );
    match input.to {
        Some(to) => {
            out.push(IC_TX_TO_FLAG_SOME);
            out.extend_from_slice(&to);
        }
        None => {
            out.push(IC_TX_TO_FLAG_NONE);
        }
    }
    out.extend_from_slice(&input.value);
    out.extend_from_slice(&input.gas_limit.to_be_bytes());
    out.extend_from_slice(&input.nonce.to_be_bytes());
    out.extend_from_slice(&input.max_fee_per_gas.to_be_bytes());
    out.extend_from_slice(&input.max_priority_fee_per_gas.to_be_bytes());
    out.extend_from_slice(
        &u32::try_from(input.data.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    out.extend_from_slice(&input.data);
    out
}

pub fn decode_ic_synthetic(caller: RevmAddress, bytes: &[u8]) -> Result<TxEnv, DecodeError> {
    let header = decode_ic_synthetic_header(bytes)?;
    let tx = TxEnv {
        caller,
        gas_limit: header.gas_limit,
        gas_price: header.max_fee,
        kind: match header.to {
            Some(to) => RevmTxKind::Call(RevmAddress::from(to)),
            None => RevmTxKind::Create,
        },
        value: RevmU256::from_be_bytes(header.value),
        data: RevmBytes::from(header.data.to_vec()),
        nonce: header.nonce,
        chain_id: Some(CHAIN_ID),
        access_list: Default::default(),
        gas_priority_fee: Some(header.max_priority),
        blob_hashes: Default::default(),
        max_fee_per_blob_gas: 0,
        authorization_list: Default::default(),
        tx_type: 2,
    };
    Ok(tx)
}

pub fn decode_tx(kind: TxKind, caller: RevmAddress, bytes: &[u8]) -> Result<TxEnv, DecodeError> {
    match kind {
        TxKind::IcSynthetic => decode_ic_synthetic(caller, bytes),
        TxKind::EthSigned => decode_eth_raw_tx(bytes),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedTxView<'a> {
    pub from: [u8; 20],
    pub to: Option<[u8; 20]>,
    pub nonce: u64,
    pub value: [u8; 32],
    pub input: Cow<'a, [u8]>,
    pub gas_limit: u64,
    pub gas_price: Option<u128>,
    pub max_fee_per_gas: Option<u128>,
    pub max_priority_fee_per_gas: Option<u128>,
    pub chain_id: Option<u64>,
    pub tx_type: u8,
    pub signature_v: Option<u64>,
    pub signature_r: Option<[u8; 32]>,
    pub signature_s: Option<[u8; 32]>,
}

pub fn decode_tx_view<'a>(
    kind: TxKind,
    caller: [u8; 20],
    bytes: &'a [u8],
) -> Result<DecodedTxView<'a>, DecodeError> {
    match kind {
        TxKind::IcSynthetic => {
            let header = decode_ic_synthetic_header(bytes)?;
            Ok(DecodedTxView {
                from: caller,
                to: header.to,
                nonce: header.nonce,
                value: header.value,
                input: Cow::Borrowed(header.data),
                gas_limit: header.gas_limit,
                gas_price: None,
                max_fee_per_gas: Some(header.max_fee),
                max_priority_fee_per_gas: Some(header.max_priority),
                chain_id: Some(CHAIN_ID),
                tx_type: 2,
                signature_v: None,
                signature_r: None,
                signature_s: None,
            })
        }
        TxKind::EthSigned => {
            let recovered = decode_eth_raw_tx_to_recovered(bytes)?;
            Ok(recovered_to_decoded_view(&recovered))
        }
    }
}

pub fn decode_eth_raw_tx(bytes: &[u8]) -> Result<TxEnv, DecodeError> {
    let recovered = decode_eth_raw_tx_to_recovered(bytes)?;
    Ok(decoded_to_tx_env(&recovered))
}

fn decode_eth_raw_tx_to_recovered(bytes: &[u8]) -> Result<RecoveredTx, DecodeError> {
    if bytes.is_empty() {
        return Err(DecodeError::InvalidLength);
    }
    if bytes.len() > MAX_TX_SIZE {
        return Err(DecodeError::DataTooLarge);
    }
    if should_reject_unsupported_typed_tx(bytes[0]) {
        return Err(DecodeError::UnsupportedType);
    }

    recover_eth_tx(bytes).map_err(map_recovery_error)
}

fn should_reject_unsupported_typed_tx(first_byte: u8) -> bool {
    first_byte == TX_TYPE_EIP4844 || first_byte == TX_TYPE_EIP7702
}

fn decoded_to_tx_env(decoded: &RecoveredTx) -> TxEnv {
    let gas_price = decoded.gas_price.or(decoded.max_fee_per_gas).unwrap_or(0);
    let gas_priority_fee = decoded.max_priority_fee_per_gas;
    let kind = match decoded.to {
        Some(addr) => RevmTxKind::Call(revm_address_from_alloy(addr)),
        None => RevmTxKind::Create,
    };
    TxEnv {
        caller: revm_address_from_alloy(decoded.from),
        gas_limit: decoded.gas_limit,
        gas_price,
        kind,
        value: RevmU256::from_be_bytes(decoded.value.to_be_bytes::<32>()),
        data: RevmBytes::from(decoded.input.clone()),
        nonce: decoded.nonce,
        chain_id: decoded.chain_id,
        access_list: AccessList(
            decoded
                .access_list
                .iter()
                .map(|item| AccessListItem {
                    address: RevmAddress::from(item.address),
                    storage_keys: item.storage_keys.iter().copied().map(B256::from).collect(),
                })
                .collect(),
        ),
        gas_priority_fee,
        blob_hashes: Vec::new(),
        max_fee_per_blob_gas: 0,
        authorization_list: Vec::new(),
        tx_type: decoded.tx_type,
    }
}

fn recovered_to_decoded_view(decoded: &RecoveredTx) -> DecodedTxView<'static> {
    let mut from = [0u8; 20];
    from.copy_from_slice(decoded.from.as_ref());
    let to = decoded.to.map(|addr| {
        let mut out = [0u8; 20];
        out.copy_from_slice(addr.as_ref());
        out
    });
    DecodedTxView {
        from,
        to,
        nonce: decoded.nonce,
        value: decoded.value.to_be_bytes(),
        input: Cow::Owned(decoded.input.clone()),
        gas_limit: decoded.gas_limit,
        gas_price: decoded.gas_price,
        max_fee_per_gas: decoded.max_fee_per_gas,
        max_priority_fee_per_gas: decoded.max_priority_fee_per_gas,
        chain_id: decoded.chain_id,
        tx_type: decoded.tx_type,
        signature_v: Some(decoded.signature_v),
        signature_r: Some(decoded.signature_r),
        signature_s: Some(decoded.signature_s),
    }
}

fn map_recovery_error(error: RecoveryError) -> DecodeError {
    match error {
        RecoveryError::UnsupportedType => DecodeError::UnsupportedType,
        RecoveryError::LegacyChainIdMissing => DecodeError::LegacyChainIdMissing,
        RecoveryError::WrongChainId => DecodeError::WrongChainId,
        RecoveryError::InvalidSignature => DecodeError::InvalidSignature,
        RecoveryError::InvalidRlp => DecodeError::InvalidRlp,
        RecoveryError::TrailingBytes => DecodeError::TrailingBytes,
    }
}

fn revm_address_from_alloy(addr: AlloyAddress) -> RevmAddress {
    RevmAddress::from_slice(addr.as_slice())
}

#[cfg(test)]
#[path = "tx_decode_tests.rs"]
mod tests;
