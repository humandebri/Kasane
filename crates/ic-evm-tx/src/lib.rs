//! どこで: ic-evm-tx の Eth署名復元境界
//! 何を: alloy-consensus/k256 依存を tx 専用crateに隔離
//! なぜ: 依存汚染範囲を最小化し、core から重い依存を切り離すため

use alloy_consensus::transaction::SignerRecoverable;
use alloy_consensus::{Signed, Transaction, TxEip1559, TxEip2930, TxLegacy};
use alloy_primitives::{Address as AlloyAddress, TxKind as AlloyTxKind, U256 as AlloyU256};
use evm_db::chain_data::constants::CHAIN_ID;

const TX_TYPE_EIP2930: u8 = 0x01;
const TX_TYPE_EIP1559: u8 = 0x02;
const TX_TYPE_EIP4844: u8 = 0x03;
const TX_TYPE_EIP7702: u8 = 0x04;
const TX_TYPE_LEGACY_ENVELOPE: u8 = 0x00;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryError {
    UnsupportedType,
    LegacyChainIdMissing,
    WrongChainId,
    InvalidSignature,
    InvalidRlp,
    TrailingBytes,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveredAccessListItem {
    pub address: [u8; 20],
    pub storage_keys: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveredTx {
    pub from: AlloyAddress,
    pub to: Option<AlloyAddress>,
    pub nonce: u64,
    pub value: AlloyU256,
    pub input: Vec<u8>,
    pub gas_limit: u64,
    pub gas_price: Option<u128>,
    pub max_fee_per_gas: Option<u128>,
    pub max_priority_fee_per_gas: Option<u128>,
    pub chain_id: Option<u64>,
    pub tx_type: u8,
    pub access_list: Vec<RecoveredAccessListItem>,
    pub signature_v: u64,
    pub signature_r: [u8; 32],
    pub signature_s: [u8; 32],
}

pub fn recover_eth_tx(bytes: &[u8]) -> Result<RecoveredTx, RecoveryError> {
    let Some(first_byte) = bytes.first().copied() else {
        return Err(RecoveryError::InvalidRlp);
    };

    if first_byte >= 0xc0 {
        return recover_legacy_untagged(bytes);
    }

    match first_byte {
        TX_TYPE_EIP2930 => recover_eip2930(bytes),
        TX_TYPE_EIP1559 => recover_eip1559(bytes),
        TX_TYPE_LEGACY_ENVELOPE | TX_TYPE_EIP4844 | TX_TYPE_EIP7702 => {
            Err(RecoveryError::UnsupportedType)
        }
        _ => Err(RecoveryError::UnsupportedType),
    }
}

fn recovered_from_tx<T: Transaction>(
    tx: &T,
    from: AlloyAddress,
    tx_type: u8,
    signature: &alloy_primitives::Signature,
) -> RecoveredTx {
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
    let chain_id = tx.chain_id();
    let signature_v = match tx_type {
        TX_TYPE_LEGACY_ENVELOPE => chain_id
            .map(|id| {
                id.saturating_mul(2)
                    .saturating_add(35 + u64::from(signature.v()))
            })
            .unwrap_or(27 + u64::from(signature.v())),
        _ => u64::from(signature.v()),
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
        chain_id,
        tx_type,
        access_list: tx
            .access_list()
            .map(|list| {
                list.iter()
                    .map(|item| {
                        let mut address = [0u8; 20];
                        address.copy_from_slice(item.address.as_ref());
                        let storage_keys = item
                            .storage_keys
                            .iter()
                            .map(|key| {
                                let mut out = [0u8; 32];
                                out.copy_from_slice(key.as_ref());
                                out
                            })
                            .collect();
                        RecoveredAccessListItem {
                            address,
                            storage_keys,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default(),
        signature_v,
        signature_r: signature.r().to_be_bytes(),
        signature_s: signature.s().to_be_bytes(),
    }
}

fn recover_legacy_untagged(bytes: &[u8]) -> Result<RecoveredTx, RecoveryError> {
    let mut buf = bytes;
    let signed = Signed::<TxLegacy>::rlp_decode(&mut buf).map_err(|_| RecoveryError::InvalidRlp)?;
    finalize_recovered(signed, TX_TYPE_LEGACY_ENVELOPE, buf)
}

fn recover_eip2930(bytes: &[u8]) -> Result<RecoveredTx, RecoveryError> {
    let mut buf = bytes;
    let signed =
        Signed::<TxEip2930>::eip2718_decode(&mut buf).map_err(|_| RecoveryError::InvalidRlp)?;
    finalize_recovered(signed, TX_TYPE_EIP2930, buf)
}

fn recover_eip1559(bytes: &[u8]) -> Result<RecoveredTx, RecoveryError> {
    let mut buf = bytes;
    let signed =
        Signed::<TxEip1559>::eip2718_decode(&mut buf).map_err(|_| RecoveryError::InvalidRlp)?;
    finalize_recovered(signed, TX_TYPE_EIP1559, buf)
}

fn finalize_recovered<T: Transaction>(
    signed: Signed<T>,
    tx_type: u8,
    trailing: &[u8],
) -> Result<RecoveredTx, RecoveryError>
where
    Signed<T>: SignerRecoverable,
{
    if !trailing.is_empty() {
        return Err(RecoveryError::TrailingBytes);
    }
    validate_chain_id(signed.tx().chain_id())?;
    let from = signed
        .recover_signer()
        .map_err(|_| RecoveryError::InvalidSignature)?;
    Ok(recovered_from_tx(
        signed.tx(),
        from,
        tx_type,
        signed.signature(),
    ))
}

fn validate_chain_id(chain_id: Option<u64>) -> Result<(), RecoveryError> {
    match chain_id {
        None => Err(RecoveryError::LegacyChainIdMissing),
        Some(id) if id != CHAIN_ID => Err(RecoveryError::WrongChainId),
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{recover_eth_tx, RecoveryError, CHAIN_ID, TX_TYPE_LEGACY_ENVELOPE};
    use alloy_consensus::{SignableTransaction, Signed, TxEip1559, TxEip2930, TxLegacy};
    use alloy_primitives::{Address, Signature, TxKind, B256, U256};

    #[test]
    fn rejects_unsupported_types_without_deep_decode() {
        assert_eq!(
            recover_eth_tx(&[TX_TYPE_LEGACY_ENVELOPE]),
            Err(RecoveryError::UnsupportedType)
        );
        assert_eq!(recover_eth_tx(&[0x03]), Err(RecoveryError::UnsupportedType));
        assert_eq!(recover_eth_tx(&[0x04]), Err(RecoveryError::UnsupportedType));
    }

    #[test]
    fn legacy_without_chain_id_is_rejected() {
        let tx = TxLegacy {
            chain_id: None,
            nonce: 1,
            gas_price: 100,
            gas_limit: 21_000,
            to: TxKind::Call(Address::ZERO),
            value: U256::ZERO,
            input: Vec::new().into(),
        };
        let raw = encode_legacy(tx.into_signed(zero_signature()));
        assert_eq!(
            recover_eth_tx(&raw),
            Err(RecoveryError::LegacyChainIdMissing)
        );
    }

    #[test]
    fn supported_types_decode_and_fail_on_invalid_signature() {
        let legacy = TxLegacy {
            chain_id: Some(CHAIN_ID),
            nonce: 1,
            gas_price: 100,
            gas_limit: 21_000,
            to: TxKind::Call(Address::ZERO),
            value: U256::from(1u64),
            input: vec![1, 2, 3].into(),
        };
        let legacy_raw = encode_legacy(legacy.into_signed(zero_signature()));
        assert_eq!(
            recover_eth_tx(&legacy_raw),
            Err(RecoveryError::InvalidSignature)
        );

        let mut tagged_legacy_raw = vec![TX_TYPE_LEGACY_ENVELOPE];
        tagged_legacy_raw.extend_from_slice(&legacy_raw);
        assert_eq!(
            recover_eth_tx(&tagged_legacy_raw),
            Err(RecoveryError::UnsupportedType)
        );

        let eip2930 = TxEip2930 {
            chain_id: CHAIN_ID,
            nonce: 7,
            gas_price: 101,
            gas_limit: 30_000,
            to: TxKind::Call(Address::ZERO),
            value: U256::from(2u64),
            input: vec![4, 5].into(),
            access_list: Default::default(),
        };
        let eip2930_raw = encode_eip2718(eip2930.into_signed(zero_signature()));
        assert_eq!(
            recover_eth_tx(&eip2930_raw),
            Err(RecoveryError::InvalidSignature)
        );

        let eip1559 = TxEip1559 {
            chain_id: CHAIN_ID,
            nonce: 8,
            max_fee_per_gas: 120,
            max_priority_fee_per_gas: 11,
            gas_limit: 31_000,
            to: TxKind::Call(Address::ZERO),
            value: U256::from(3u64),
            input: vec![6, 7].into(),
            access_list: Default::default(),
        };
        let eip1559_raw = encode_eip2718(eip1559.into_signed(zero_signature()));
        assert_eq!(
            recover_eth_tx(&eip1559_raw),
            Err(RecoveryError::InvalidSignature)
        );
    }

    fn zero_signature() -> Signature {
        Signature::from_scalars_and_parity(B256::ZERO, B256::ZERO, false)
    }

    fn encode_legacy(signed: Signed<TxLegacy>) -> Vec<u8> {
        let mut out = Vec::new();
        signed.rlp_encode(&mut out);
        out
    }

    fn encode_eip2718<T>(signed: Signed<T>) -> Vec<u8>
    where
        T: alloy_consensus::transaction::RlpEcdsaEncodableTx,
    {
        let mut out = Vec::new();
        signed.eip2718_encode(&mut out);
        out
    }
}
