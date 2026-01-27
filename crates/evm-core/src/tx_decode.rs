//! どこで: Phase1のTxデコード / 何を: IcSyntheticの最小フォーマット / なぜ: 決定的な解釈のため

use evm_backend::phase1::constants::MAX_TX_SIZE;
use evm_backend::phase1::TxKind;
use revm::primitives::{Address, Bytes, TxKind as RevmTxKind, U256};
use revm::context::TxEnv;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecodeError {
    InvalidLength,
    InvalidVersion,
    DataTooLarge,
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
    let data_len = u32::from_be_bytes(len) as usize;
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
        chain_id: None,
        access_list: Default::default(),
        gas_priority_fee: None,
        blob_hashes: Default::default(),
        max_fee_per_blob_gas: 0,
        authorization_list: Default::default(),
        tx_type: 0,
        ..Default::default()
    };
    Ok(tx)
}

pub fn decode_tx(kind: TxKind, caller: Address, bytes: &[u8]) -> Result<TxEnv, DecodeError> {
    match kind {
        TxKind::IcSynthetic => decode_ic_synthetic(caller, bytes),
        TxKind::EthSigned => Err(DecodeError::InvalidVersion),
    }
}
