//! どこで: Phase1のOpDeposit decode / 何を: wire v1 を検証して TxEnv 化 / なぜ: PR2で入力検証を先行するため

use crate::tx_decode::{DecodeError, DepositInvalidReason};
use evm_db::chain_data::constants::{CHAIN_ID, MAX_TX_SIZE};
use op_revm::transaction::OpTransaction;
use revm::context::TxEnv;
use revm::primitives::{Address, Bytes, TxKind, B256, U256};

const OP_DEPOSIT_WIRE_V1: u8 = 1;
// version + source_hash + from + to_flag + mint + value + gas_limit + is_system + data_len
const OP_DEPOSIT_MIN_LEN: usize = 1 + 32 + 20 + 1 + 32 + 32 + 8 + 1 + 4;

pub fn decode_op_deposit(bytes: &[u8]) -> Result<TxEnv, DecodeError> {
    let wire = OpDepositWireV1::decode(bytes)?;
    wire.validate()?;
    wire.validate_with_op_revm()?;
    Ok(wire.to_tx_env())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpDepositWireV1 {
    source_hash: [u8; 32],
    from: [u8; 20],
    to: Option<[u8; 20]>,
    mint: [u8; 32],
    value: [u8; 32],
    gas_limit: u64,
    is_system_transaction: bool,
    data: Vec<u8>,
}

impl OpDepositWireV1 {
    fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() < OP_DEPOSIT_MIN_LEN {
            return Err(DecodeError::DepositInvalid(
                DepositInvalidReason::LengthMismatch,
            ));
        }
        let mut cursor = 0usize;
        let version = bytes[cursor];
        cursor += 1;
        if version != OP_DEPOSIT_WIRE_V1 {
            return Err(DecodeError::DepositInvalid(
                DepositInvalidReason::VersionMismatch,
            ));
        }

        let source_hash = read_fixed_32(bytes, &mut cursor).ok_or(DecodeError::DepositInvalid(
            DepositInvalidReason::LengthMismatch,
        ))?;
        let from = read_fixed_20(bytes, &mut cursor).ok_or(DecodeError::DepositInvalid(
            DepositInvalidReason::LengthMismatch,
        ))?;

        let to_flag = *bytes.get(cursor).ok_or(DecodeError::DepositInvalid(
            DepositInvalidReason::LengthMismatch,
        ))?;
        cursor += 1;
        let to = match to_flag {
            0 => None,
            1 => Some(
                read_fixed_20(bytes, &mut cursor).ok_or(DecodeError::DepositInvalid(
                    DepositInvalidReason::LengthMismatch,
                ))?,
            ),
            _ => return Err(DecodeError::DepositInvalid(DepositInvalidReason::BadToFlag)),
        };

        let mint = read_fixed_32(bytes, &mut cursor).ok_or(DecodeError::DepositInvalid(
            DepositInvalidReason::LengthMismatch,
        ))?;
        let value = read_fixed_32(bytes, &mut cursor).ok_or(DecodeError::DepositInvalid(
            DepositInvalidReason::LengthMismatch,
        ))?;
        let gas_limit = read_u64_be(bytes, &mut cursor).ok_or(DecodeError::DepositInvalid(
            DepositInvalidReason::LengthMismatch,
        ))?;
        let is_system = *bytes.get(cursor).ok_or(DecodeError::DepositInvalid(
            DepositInvalidReason::LengthMismatch,
        ))?;
        cursor += 1;
        let is_system_transaction = match is_system {
            0 => false,
            // PR2方針: Regolith以降を前提にし、wireからのsystem depositは許可しない。
            1 => {
                return Err(DecodeError::DepositInvalid(
                    DepositInvalidReason::BadIsSystemFlag,
                ))
            }
            _ => {
                return Err(DecodeError::DepositInvalid(
                    DepositInvalidReason::BadIsSystemFlag,
                ))
            }
        };

        let data_len = read_u32_be(bytes, &mut cursor).ok_or(DecodeError::DepositInvalid(
            DepositInvalidReason::LengthMismatch,
        ))? as usize;
        if data_len > MAX_TX_SIZE {
            return Err(DecodeError::DepositInvalid(
                DepositInvalidReason::DataTooLarge,
            ));
        }
        let end = cursor.saturating_add(data_len);
        if end != bytes.len() {
            return Err(DecodeError::DepositInvalid(
                DepositInvalidReason::LengthMismatch,
            ));
        }
        let data = bytes[cursor..end].to_vec();

        Ok(Self {
            source_hash,
            from,
            to,
            mint,
            value,
            gas_limit,
            is_system_transaction,
            data,
        })
    }

    fn validate(&self) -> Result<(), DecodeError> {
        if self.source_hash == [0u8; 32] {
            return Err(DecodeError::DepositInvalid(
                DepositInvalidReason::SourceHashZero,
            ));
        }
        Ok(())
    }

    fn validate_with_op_revm(&self) -> Result<(), DecodeError> {
        let mut op_tx_builder = OpTransaction::builder()
            .base(revm::context::TxEnv::builder().tx_type(Some(0x7e)))
            .source_hash(B256::from(self.source_hash));

        if self.is_system_transaction {
            op_tx_builder = op_tx_builder.is_system_transaction();
        } else {
            op_tx_builder = op_tx_builder.not_system_transaction();
        }

        let mint_u256 = U256::from_be_bytes(self.mint);
        if mint_u256 != U256::ZERO {
            let mint_u128 = u128::try_from(mint_u256).map_err(|_| {
                DecodeError::DepositInvalid(DepositInvalidReason::MintTooLargeForOpRevm)
            })?;
            op_tx_builder = op_tx_builder.mint(mint_u128);
        }

        op_tx_builder.build().map_err(|_| {
            DecodeError::DepositInvalid(DepositInvalidReason::OpRevmValidationFailed)
        })?;
        Ok(())
    }

    fn to_tx_env(&self) -> TxEnv {
        TxEnv {
            tx_type: 0x7e,
            caller: Address::from(self.from),
            gas_limit: self.gas_limit,
            gas_price: 0,
            kind: match self.to {
                Some(to) => TxKind::Call(Address::from(to)),
                None => TxKind::Create,
            },
            value: U256::from_be_bytes(self.value),
            data: Bytes::from(self.data.clone()),
            nonce: 0,
            chain_id: Some(CHAIN_ID),
            access_list: Default::default(),
            gas_priority_fee: None,
            blob_hashes: Vec::new(),
            max_fee_per_blob_gas: 0,
            authorization_list: Vec::new(),
        }
    }
}

fn read_fixed_20(bytes: &[u8], cursor: &mut usize) -> Option<[u8; 20]> {
    let end = cursor.checked_add(20)?;
    let chunk = bytes.get(*cursor..end)?;
    let mut out = [0u8; 20];
    out.copy_from_slice(chunk);
    *cursor = end;
    Some(out)
}

fn read_fixed_32(bytes: &[u8], cursor: &mut usize) -> Option<[u8; 32]> {
    let end = cursor.checked_add(32)?;
    let chunk = bytes.get(*cursor..end)?;
    let mut out = [0u8; 32];
    out.copy_from_slice(chunk);
    *cursor = end;
    Some(out)
}

fn read_u32_be(bytes: &[u8], cursor: &mut usize) -> Option<u32> {
    let end = cursor.checked_add(4)?;
    let chunk = bytes.get(*cursor..end)?;
    let mut raw = [0u8; 4];
    raw.copy_from_slice(chunk);
    *cursor = end;
    Some(u32::from_be_bytes(raw))
}

fn read_u64_be(bytes: &[u8], cursor: &mut usize) -> Option<u64> {
    let end = cursor.checked_add(8)?;
    let chunk = bytes.get(*cursor..end)?;
    let mut raw = [0u8; 8];
    raw.copy_from_slice(chunk);
    *cursor = end;
    Some(u64::from_be_bytes(raw))
}
