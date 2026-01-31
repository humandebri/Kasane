//! どこで: Phase1のTxデコード / 何を: IcSynthetic + Eth raw tx の安全なデコード / なぜ: 互換性とtrap回避

use alloy_consensus::transaction::SignerRecoverable;
use alloy_consensus::{Transaction, TxEnvelope, TxType};
use alloy_eips::eip2718::{Decodable2718, Eip2718Error};
use alloy_primitives::{Address as AlloyAddress, TxKind as AlloyTxKind, B256, U256 as AlloyU256};
use evm_db::chain_data::constants::{CHAIN_ID, MAX_TX_SIZE};
use evm_db::chain_data::TxKind;
use revm::context::TxEnv;
use revm::primitives::{
    Address as RevmAddress, Bytes as RevmBytes, TxKind as RevmTxKind, U256 as RevmU256,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecodeError {
    InvalidLength,
    InvalidVersion,
    DataTooLarge,
    UnsupportedType,
    InvalidSignature,
    InvalidRlp,
    TrailingBytes,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DecodedTx {
    from: AlloyAddress,
    to: Option<AlloyAddress>,
    nonce: u64,
    value: AlloyU256,
    input: Vec<u8>,
    gas_limit: u64,
    gas_price: Option<u128>,
    max_fee_per_gas: Option<u128>,
    max_priority_fee_per_gas: Option<u128>,
    chain_id: Option<u64>,
    tx_hash: [u8; 32],
    tx_type: u8,
}

// IcSynthetic v2: [version:1][to:20][value:32][gas_limit:8][nonce:8]
//                [max_fee_per_gas:16][max_priority_fee_per_gas:16][data_len:4][data]
const IC_TX_VERSION: u8 = 2;
const IC_TX_HEADER_LEN: usize = 1 + 20 + 32 + 8 + 8 + 16 + 16 + 4;

pub fn decode_ic_synthetic(caller: RevmAddress, bytes: &[u8]) -> Result<TxEnv, DecodeError> {
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
    let mut max_fee = [0u8; 16];
    max_fee.copy_from_slice(&bytes[offset..offset + 16]);
    offset += 16;
    let mut max_priority = [0u8; 16];
    max_priority.copy_from_slice(&bytes[offset..offset + 16]);
    offset += 16;
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
        gas_price: u128::from_be_bytes(max_fee),
        kind: RevmTxKind::Call(RevmAddress::from(to)),
        value: RevmU256::from_be_bytes(value),
        data: RevmBytes::from(data),
        nonce: u64::from_be_bytes(nonce),
        chain_id: Some(CHAIN_ID),
        access_list: Default::default(),
        gas_priority_fee: Some(u128::from_be_bytes(max_priority)),
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
pub struct DecodedTxView {
    pub from: [u8; 20],
    pub to: Option<[u8; 20]>,
    pub nonce: u64,
    pub value: [u8; 32],
    pub input: Vec<u8>,
    pub gas_limit: u64,
    pub gas_price: u128,
    pub chain_id: Option<u64>,
}

pub fn decode_tx_view(
    kind: TxKind,
    caller: [u8; 20],
    bytes: &[u8],
) -> Result<DecodedTxView, DecodeError> {
    match kind {
        TxKind::EthSigned => {
            let decoded = decode_eth_raw_tx_to_decoded(bytes)?;
            Ok(DecodedTxView {
                from: address_to_array(decoded.from),
                to: decoded.to.map(address_to_array),
                nonce: decoded.nonce,
                value: decoded.value.to_be_bytes(),
                input: decoded.input,
                gas_limit: decoded.gas_limit,
                gas_price: decoded.gas_price.or(decoded.max_fee_per_gas).unwrap_or(0),
                chain_id: decoded.chain_id,
            })
        }
        TxKind::IcSynthetic => {
            let tx_env = decode_ic_synthetic(RevmAddress::from(caller), bytes)?;
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
                input: tx_env.data.to_vec(),
                gas_limit: tx_env.gas_limit,
                gas_price: tx_env.gas_price,
                chain_id: tx_env.chain_id,
            })
        }
    }
}

pub fn decode_eth_raw_tx(bytes: &[u8]) -> Result<TxEnv, DecodeError> {
    let decoded = decode_eth_raw_tx_to_decoded(bytes)?;
    Ok(decoded_to_tx_env(&decoded))
}

fn decode_eth_raw_tx_to_decoded(bytes: &[u8]) -> Result<DecodedTx, DecodeError> {
    if bytes.is_empty() {
        return Err(DecodeError::InvalidLength);
    }
    if bytes.len() > MAX_TX_SIZE {
        return Err(DecodeError::DataTooLarge);
    }
    let envelope = TxEnvelope::decode_2718_exact(bytes).map_err(map_eip2718_error)?;
    let sender = envelope
        .recover_signer()
        .map_err(|_| DecodeError::InvalidSignature)?;
    let tx_hash = *envelope.tx_hash();

    match envelope {
        TxEnvelope::Legacy(tx) => Ok(decoded_from_tx(
            tx.tx(),
            sender,
            tx_hash,
            TxType::Legacy as u8,
        )),
        TxEnvelope::Eip2930(tx) => Ok(decoded_from_tx(
            tx.tx(),
            sender,
            tx_hash,
            TxType::Eip2930 as u8,
        )),
        TxEnvelope::Eip1559(tx) => Ok(decoded_from_tx(
            tx.tx(),
            sender,
            tx_hash,
            TxType::Eip1559 as u8,
        )),
        TxEnvelope::Eip4844(_) | TxEnvelope::Eip7702(_) => Err(DecodeError::UnsupportedType),
    }
}

fn decoded_from_tx<T: Transaction>(
    tx: &T,
    from: AlloyAddress,
    tx_hash: B256,
    tx_type: u8,
) -> DecodedTx {
    let to = match tx.kind() {
        AlloyTxKind::Call(addr) => Some(addr),
        AlloyTxKind::Create => None,
    };
    let is_dynamic_fee = tx.is_dynamic_fee();
    let gas_price = if is_dynamic_fee { None } else { tx.gas_price() };
    let max_fee_per_gas = if is_dynamic_fee { Some(tx.max_fee_per_gas()) } else { None };
    let max_priority_fee_per_gas = if is_dynamic_fee {
        tx.max_priority_fee_per_gas()
    } else {
        None
    };
    DecodedTx {
        from,
        to,
        nonce: tx.nonce(),
        value: tx.value(),
        input: tx.input().to_vec(),
        gas_limit: tx.gas_limit(),
        gas_price,
        max_fee_per_gas,
        max_priority_fee_per_gas,
        chain_id: tx.chain_id().map(|id| id),
        tx_hash: tx_hash.0,
        tx_type,
    }
}

fn decoded_to_tx_env(decoded: &DecodedTx) -> TxEnv {
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
        blob_hashes: Default::default(),
        max_fee_per_blob_gas: 0,
        authorization_list: Default::default(),
        tx_type: decoded.tx_type,
    }
}

fn map_eip2718_error(error: Eip2718Error) -> DecodeError {
    match error {
        Eip2718Error::UnexpectedType(_) => DecodeError::UnsupportedType,
        Eip2718Error::RlpError(alloy_rlp::Error::UnexpectedLength) => DecodeError::TrailingBytes,
        Eip2718Error::RlpError(_) => DecodeError::InvalidRlp,
        _ => DecodeError::InvalidRlp,
    }
}

fn revm_address_from_alloy(addr: AlloyAddress) -> RevmAddress {
    RevmAddress::from_slice(addr.as_slice())
}

fn address_to_array(addr: AlloyAddress) -> [u8; 20] {
    let mut out = [0u8; 20];
    out.copy_from_slice(addr.as_slice());
    out
}
