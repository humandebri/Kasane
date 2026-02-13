//! どこで: Phase1のTxデコード / 何を: IcSynthetic + Eth の安全なデコード / なぜ: 互換性とtrap回避
use alloy_primitives::Address as AlloyAddress;
use byteorder::{BigEndian, ByteOrder};
use evm_db::chain_data::constants::{CHAIN_ID, MAX_TX_SIZE};
use evm_db::chain_data::TxKind;
use evm_tx::{recover_eth_tx, RecoveredTx, RecoveryError};
use revm::context::TxEnv;
use revm::primitives::{
    Address as RevmAddress, Bytes as RevmBytes, TxKind as RevmTxKind, U256 as RevmU256,
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

// IcSynthetic v2: [version:1][to:20][value:32][gas_limit:8][nonce:8]
//                [max_fee_per_gas:16][max_priority_fee_per_gas:16][data_len:4][data]
const IC_TX_VERSION: u8 = 2;
const IC_TX_HEADER_LEN: usize = 1 + 20 + 32 + 8 + 8 + 16 + 16 + 4;
const TX_TYPE_EIP4844: u8 = 0x03;
const TX_TYPE_EIP7702: u8 = 0x04;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IcTxHeader<'a> {
    pub to: [u8; 20],
    pub value: [u8; 32],
    pub gas_limit: u64,
    pub nonce: u64,
    pub max_fee: u128,
    pub max_priority: u128,
    pub data: &'a [u8],
}

pub fn decode_ic_synthetic_header(bytes: &[u8]) -> Result<IcTxHeader<'_>, DecodeError> {
    decode_ic_synthetic_header_impl(bytes, true)
}

pub(crate) fn decode_ic_synthetic_header_trusted_size(
    bytes: &[u8],
) -> Result<IcTxHeader<'_>, DecodeError> {
    decode_ic_synthetic_header_impl(bytes, false)
}

fn decode_ic_synthetic_header_impl(
    bytes: &[u8],
    enforce_data_size_limit: bool,
) -> Result<IcTxHeader<'_>, DecodeError> {
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
    let gas_limit = BigEndian::read_u64(&bytes[offset..offset + 8]);
    offset += 8;
    let nonce = BigEndian::read_u64(&bytes[offset..offset + 8]);
    offset += 8;
    let max_fee = BigEndian::read_u128(&bytes[offset..offset + 16]);
    offset += 16;
    let max_priority = BigEndian::read_u128(&bytes[offset..offset + 16]);
    offset += 16;
    let data_len = usize::try_from(BigEndian::read_u32(&bytes[offset..offset + 4]))
        .map_err(|_| DecodeError::InvalidLength)?;
    offset += 4;
    let expected = IC_TX_HEADER_LEN
        .checked_add(data_len)
        .ok_or(DecodeError::InvalidLength)?;
    if expected != bytes.len() {
        return Err(DecodeError::InvalidLength);
    }
    if enforce_data_size_limit && data_len > MAX_TX_SIZE {
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

pub fn decode_ic_synthetic(caller: RevmAddress, bytes: &[u8]) -> Result<TxEnv, DecodeError> {
    let header = decode_ic_synthetic_header(bytes)?;
    let tx = TxEnv {
        caller,
        gas_limit: header.gas_limit,
        gas_price: header.max_fee,
        kind: RevmTxKind::Call(RevmAddress::from(header.to)),
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
    pub gas_price: u128,
    pub chain_id: Option<u64>,
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
                to: Some(header.to),
                nonce: header.nonce,
                value: header.value,
                input: Cow::Borrowed(header.data),
                gas_limit: header.gas_limit,
                gas_price: header.max_fee,
                chain_id: Some(CHAIN_ID),
            })
        }
        TxKind::EthSigned => {
            let tx_env = decode_tx(kind, RevmAddress::from(caller), bytes)?;
            let to = match tx_env.kind {
                RevmTxKind::Call(addr) => {
                    let mut out = [0u8; 20];
                    out.copy_from_slice(addr.as_ref());
                    Some(out)
                }
                RevmTxKind::Create => None,
            };
            let mut from = [0u8; 20];
            from.copy_from_slice(tx_env.caller.as_ref());
            Ok(DecodedTxView {
                from,
                to,
                nonce: tx_env.nonce,
                value: tx_env.value.to_be_bytes(),
                input: Cow::Owned(tx_env.data.to_vec()),
                gas_limit: tx_env.gas_limit,
                gas_price: tx_env.gas_price,
                chain_id: tx_env.chain_id,
            })
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
        access_list: Default::default(),
        gas_priority_fee,
        blob_hashes: Vec::new(),
        max_fee_per_blob_gas: 0,
        authorization_list: Vec::new(),
        tx_type: decoded.tx_type,
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
mod tests {
    use super::{decode_eth_raw_tx, should_reject_unsupported_typed_tx, DecodeError};

    #[test]
    fn unsupported_typed_tx_prefixes_are_rejected_early() {
        assert!(should_reject_unsupported_typed_tx(0x03));
        assert!(should_reject_unsupported_typed_tx(0x04));
        assert!(!should_reject_unsupported_typed_tx(0x01));
        assert!(!should_reject_unsupported_typed_tx(0x02));
    }

    #[test]
    fn decode_eth_raw_tx_rejects_4844_and_7702_prefix_without_deep_decode() {
        assert_eq!(
            decode_eth_raw_tx(&[0x03]).err(),
            Some(DecodeError::UnsupportedType)
        );
        assert_eq!(
            decode_eth_raw_tx(&[0x04]).err(),
            Some(DecodeError::UnsupportedType)
        );
    }
}
