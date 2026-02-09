//! どこで: evm-core の Eth署名復元境界
//! 何を: alloy-consensus/k256 依存を1箇所に隔離
//! なぜ: 依存汚染範囲を最小化して将来分離を容易にするため

use alloy_consensus::transaction::SignerRecoverable;
use alloy_consensus::{Transaction, TxEnvelope};
use alloy_eips::eip2718::{Decodable2718, Eip2718Error};
use alloy_eips::Typed2718;
use alloy_primitives::{Address as AlloyAddress, TxKind as AlloyTxKind, U256 as AlloyU256};
use evm_db::chain_data::constants::CHAIN_ID;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryError {
    UnsupportedType,
    LegacyChainIdMissing,
    WrongChainId,
    InvalidSignature,
    InvalidRlp,
    TrailingBytes,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecoveredTx {
    pub(crate) from: AlloyAddress,
    pub(crate) to: Option<AlloyAddress>,
    pub(crate) nonce: u64,
    pub(crate) value: AlloyU256,
    pub(crate) input: Vec<u8>,
    pub(crate) gas_limit: u64,
    pub(crate) gas_price: Option<u128>,
    pub(crate) max_fee_per_gas: Option<u128>,
    pub(crate) max_priority_fee_per_gas: Option<u128>,
    pub(crate) chain_id: Option<u64>,
    pub(crate) tx_type: u8,
}

pub(crate) fn recover_eth_tx(bytes: &[u8]) -> Result<RecoveredTx, RecoveryError> {
    let envelope = TxEnvelope::decode_2718_exact(bytes).map_err(map_eip2718_error)?;

    match envelope.chain_id() {
        None => return Err(RecoveryError::LegacyChainIdMissing),
        Some(chain_id) if chain_id != CHAIN_ID => return Err(RecoveryError::WrongChainId),
        _ => {}
    }

    let sender = envelope
        .recover_signer()
        .map_err(|_| RecoveryError::InvalidSignature)?;

    match envelope {
        TxEnvelope::Legacy(tx) => Ok(recovered_from_tx(tx.tx(), sender, tx.ty())),
        TxEnvelope::Eip2930(tx) => Ok(recovered_from_tx(tx.tx(), sender, tx.ty())),
        TxEnvelope::Eip1559(tx) => Ok(recovered_from_tx(tx.tx(), sender, tx.ty())),
        TxEnvelope::Eip4844(tx) => Ok(recovered_from_tx(tx.tx(), sender, tx.ty())),
        TxEnvelope::Eip7702(tx) => Ok(recovered_from_tx(tx.tx(), sender, tx.ty())),
    }
}

fn recovered_from_tx<T: Transaction>(tx: &T, from: AlloyAddress, tx_type: u8) -> RecoveredTx {
    let to = match tx.kind() {
        AlloyTxKind::Call(addr) => Some(addr),
        AlloyTxKind::Create => None,
    };
    let is_dynamic_fee = tx.is_dynamic_fee();
    let gas_price = if is_dynamic_fee { None } else { tx.gas_price() };
    let max_fee_per_gas = if is_dynamic_fee {
        Some(tx.max_fee_per_gas())
    } else {
        None
    };
    let max_priority_fee_per_gas = if is_dynamic_fee {
        tx.max_priority_fee_per_gas()
    } else {
        None
    };
    RecoveredTx {
        from,
        to,
        nonce: tx.nonce(),
        value: tx.value(),
        input: tx.input().to_vec(),
        gas_limit: tx.gas_limit(),
        gas_price,
        max_fee_per_gas,
        max_priority_fee_per_gas,
        chain_id: tx.chain_id(),
        tx_type,
    }
}

fn map_eip2718_error(error: Eip2718Error) -> RecoveryError {
    match error {
        Eip2718Error::UnexpectedType(_) => RecoveryError::UnsupportedType,
        Eip2718Error::RlpError(alloy_rlp::Error::UnexpectedLength) => RecoveryError::TrailingBytes,
        Eip2718Error::RlpError(_) => RecoveryError::InvalidRlp,
        _ => RecoveryError::InvalidRlp,
    }
}

