//! どこで: wrap/unwrap request 永続化 / 何を: request状態の最小表現 / なぜ: 非同期実行結果をupgrade後も追跡するため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

const MAX_BLOB_LEN: usize = 256;
const MAX_ERROR_LEN: usize = 192;
const MAX_LEDGER_TX_ID_LEN: usize = 128;
const MAX_ENCODED_LEN: u32 = 1_512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnwrapRequestStatus {
    Queued,
    Dispatching,
    Dispatched,
    DispatchFailed,
}

impl UnwrapRequestStatus {
    fn to_u8(self) -> u8 {
        match self {
            Self::Queued => 0,
            Self::Dispatching => 1,
            Self::Dispatched => 2,
            Self::DispatchFailed => 3,
        }
    }

    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Queued),
            1 => Some(Self::Dispatching),
            2 => Some(Self::Dispatched),
            3 => Some(Self::DispatchFailed),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnwrapRequestV1 {
    pub vault_canister_id: Vec<u8>,
    pub asset_id: Vec<u8>,
    pub amount: Vec<u8>,
    pub recipient: Vec<u8>,
    pub status: UnwrapRequestStatus,
    pub ledger_tx_id: Option<Vec<u8>>,
    pub error_code: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Storable for UnwrapRequestV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let encoded = self
            .encode_checked()
            .unwrap_or_else(|| panic!("unwrap_request.encode_failed"));
        encode_guarded(b"unwrap_request", Cow::Owned(encoded), MAX_ENCODED_LEN)
            .unwrap_or_else(|_| panic!("unwrap_request.encode_guard_failed"))
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        match Self::decode_checked(bytes.as_ref()) {
            Some(value) => value,
            None => {
                mark_decode_failure(b"unwrap_request", false);
                Self::decode_failed_sentinel()
            }
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: MAX_ENCODED_LEN,
        is_fixed_size: false,
    };
}

impl UnwrapRequestV1 {
    fn decode_failed_sentinel() -> Self {
        Self {
            vault_canister_id: vec![0u8],
            asset_id: vec![0u8],
            amount: vec![0u8; 32],
            recipient: vec![0u8],
            status: UnwrapRequestStatus::DispatchFailed,
            ledger_tx_id: None,
            error_code: Some("stable.decode.unwrap_request".to_string()),
            created_at: 0,
            updated_at: 0,
        }
    }

    fn encode_checked(&self) -> Option<Vec<u8>> {
        if self.vault_canister_id.is_empty()
            || self.vault_canister_id.len() > MAX_BLOB_LEN
            || self.asset_id.is_empty()
            || self.asset_id.len() > MAX_BLOB_LEN
            || self.amount.len() != 32
            || self.recipient.is_empty()
            || self.recipient.len() > MAX_BLOB_LEN
        {
            return None;
        }
        if self
            .ledger_tx_id
            .as_ref()
            .is_some_and(|v| v.len() > MAX_LEDGER_TX_ID_LEN)
        {
            return None;
        }
        if self
            .error_code
            .as_ref()
            .is_some_and(|v| v.len() > MAX_ERROR_LEN)
        {
            return None;
        }
        let mut out = Vec::with_capacity(256);
        out.push(1u8);
        write_bytes(&mut out, &self.vault_canister_id)?;
        write_bytes(&mut out, &self.asset_id)?;
        write_bytes(&mut out, &self.amount)?;
        write_bytes(&mut out, &self.recipient)?;
        out.push(self.status.to_u8());
        match self.ledger_tx_id.as_ref() {
            Some(value) => {
                out.push(1u8);
                write_bytes(&mut out, value)?;
            }
            None => out.push(0u8),
        }
        match self.error_code.as_ref() {
            Some(value) => {
                out.push(1u8);
                write_bytes(&mut out, value.as_bytes())?;
            }
            None => out.push(0u8),
        }
        out.extend_from_slice(&self.created_at.to_be_bytes());
        out.extend_from_slice(&self.updated_at.to_be_bytes());
        Some(out)
    }

    fn decode_checked(data: &[u8]) -> Option<Self> {
        let mut offset = 0usize;
        let version = *data.get(offset)?;
        if version != 1 {
            return None;
        }
        offset += 1;
        let vault_canister_id = read_bytes(data, &mut offset, MAX_BLOB_LEN)?;
        let asset_id = read_bytes(data, &mut offset, MAX_BLOB_LEN)?;
        let amount = read_bytes(data, &mut offset, 32)?;
        if amount.len() != 32 {
            return None;
        }
        let recipient = read_bytes(data, &mut offset, MAX_BLOB_LEN)?;
        let status = UnwrapRequestStatus::from_u8(*data.get(offset)?)?;
        offset += 1;
        let ledger_tx_id = match *data.get(offset)? {
            0 => {
                offset += 1;
                None
            }
            1 => {
                offset += 1;
                Some(read_bytes(data, &mut offset, MAX_LEDGER_TX_ID_LEN)?)
            }
            _ => return None,
        };
        let error_code = match *data.get(offset)? {
            0 => {
                offset += 1;
                None
            }
            1 => {
                offset += 1;
                Some(String::from_utf8(read_bytes(data, &mut offset, MAX_ERROR_LEN)?).ok()?)
            }
            _ => return None,
        };
        let created_at = read_u64(data, &mut offset)?;
        let updated_at = read_u64(data, &mut offset)?;
        if offset != data.len() {
            return None;
        }
        Some(Self {
            vault_canister_id,
            asset_id,
            amount,
            recipient,
            status,
            ledger_tx_id,
            error_code,
            created_at,
            updated_at,
        })
    }
}

fn write_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Option<()> {
    let len = u16::try_from(bytes.len()).ok()?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Some(())
}

fn read_bytes(data: &[u8], offset: &mut usize, max_len: usize) -> Option<Vec<u8>> {
    let len_end = offset.checked_add(2)?;
    let len_raw = data.get(*offset..len_end)?;
    let len = u16::from_be_bytes([len_raw[0], len_raw[1]]) as usize;
    if len == 0 || len > max_len {
        return None;
    }
    *offset = len_end;
    let end = offset.checked_add(len)?;
    let out = data.get(*offset..end)?.to_vec();
    *offset = end;
    Some(out)
}

fn read_u64(data: &[u8], offset: &mut usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    let raw = data.get(*offset..end)?;
    *offset = end;
    Some(u64::from_be_bytes(raw.try_into().ok()?))
}
