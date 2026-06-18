//! どこで: ICP update intent dispatch / 何を: update要求の永続状態 / なぜ: EVM commit後の外部副作用を追跡するため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use crate::chain_data::constants::MAX_RETURN_DATA;
use crate::chain_data::tx::{TxId, TxKind};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

const MAX_TARGET_LEN: usize = 29;
const MAX_METHOD_LEN: usize = 64;
const MAX_ARG_LEN: usize = 3_997;
const MAX_ERROR_LEN: usize = 192;
const MAX_ENCODED_LEN: u32 = 38_656;
const CHECKSUM_LEN: usize = 4;
pub const MAX_ICP_UPDATE_REQUESTS: usize = 10_000;
pub const ICP_UPDATE_DECODE_FAILURE_CODE: &str = "stable.decode.icp_update_request";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IcpUpdateRequestStatus {
    Queued,
    Dispatching,
    Dispatched,
    DispatchFailed,
    DispatchUncertain,
}

impl IcpUpdateRequestStatus {
    fn to_u8(self) -> u8 {
        match self {
            Self::Queued => 0,
            Self::Dispatching => 1,
            Self::Dispatched => 2,
            Self::DispatchFailed => 3,
            Self::DispatchUncertain => 4,
        }
    }

    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Queued),
            1 => Some(Self::Dispatching),
            2 => Some(Self::Dispatched),
            3 => Some(Self::DispatchFailed),
            4 => Some(Self::DispatchUncertain),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IcpUpdateDispatchRequest {
    pub request_id: TxId,
    pub tx_id: TxId,
    pub block_number: u64,
    pub tx_index: u32,
    pub log_index: u32,
    pub tx_kind: TxKind,
    pub evm_sender: [u8; 20],
    pub ic_caller: Option<Vec<u8>>,
    pub target: Vec<u8>,
    pub method: String,
    pub arg: Vec<u8>,
    pub status: IcpUpdateRequestStatus,
    pub reply: Option<Vec<u8>>,
    pub error_code: Option<String>,
    pub updated_at: u64,
    pub call_started_at_time: u64,
}

impl Storable for IcpUpdateDispatchRequest {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let encoded = self
            .encode_checked()
            .unwrap_or_else(|| panic!("icp_update_request.encode_failed"));
        encode_guarded(b"icp_update_request", Cow::Owned(encoded), MAX_ENCODED_LEN)
            .unwrap_or_else(|_| panic!("icp_update_request.encode_guard_failed"))
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        Self::decode_checked(bytes.as_ref()).unwrap_or_else(|| {
            mark_decode_failure(b"icp_update_request", false);
            Self::decode_failure_placeholder()
        })
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: MAX_ENCODED_LEN,
        is_fixed_size: false,
    };
}

impl IcpUpdateDispatchRequest {
    fn decode_failure_placeholder() -> Self {
        Self {
            target: vec![0u8],
            method: "decode_failure".to_string(),
            arg: Vec::new(),
            request_id: TxId([0u8; 32]),
            tx_id: TxId([0u8; 32]),
            block_number: 0,
            tx_index: 0,
            log_index: 0,
            tx_kind: TxKind::EthSigned,
            evm_sender: [0u8; 20],
            ic_caller: None,
            status: IcpUpdateRequestStatus::DispatchFailed,
            reply: None,
            error_code: Some(ICP_UPDATE_DECODE_FAILURE_CODE.to_string()),
            updated_at: 0,
            call_started_at_time: 0,
        }
    }

    fn encode_checked(&self) -> Option<Vec<u8>> {
        if self.target.is_empty()
            || self.target.len() > MAX_TARGET_LEN
            || self.method.is_empty()
            || self.method.len() > MAX_METHOD_LEN
            || !self.method.is_ascii()
            || self.arg.len() > MAX_ARG_LEN
        {
            return None;
        }
        if self
            .ic_caller
            .as_ref()
            .is_some_and(|value| value.is_empty() || value.len() > MAX_TARGET_LEN)
        {
            return None;
        }
        if self
            .reply
            .as_ref()
            .is_some_and(|value| value.len() > MAX_RETURN_DATA)
        {
            return None;
        }
        if self
            .error_code
            .as_ref()
            .is_some_and(|value| value.len() > MAX_ERROR_LEN)
        {
            return None;
        }
        let mut out = Vec::with_capacity(128 + self.arg.len());
        out.push(2u8);
        out.extend_from_slice(&self.request_id.0);
        out.extend_from_slice(&self.tx_id.0);
        out.extend_from_slice(&self.block_number.to_be_bytes());
        out.extend_from_slice(&self.tx_index.to_be_bytes());
        out.extend_from_slice(&self.log_index.to_be_bytes());
        out.push(self.tx_kind.to_u8());
        out.extend_from_slice(&self.evm_sender);
        match self.ic_caller.as_ref() {
            Some(value) => {
                out.push(1u8);
                write_bytes(&mut out, value)?;
            }
            None => out.push(0u8),
        }
        write_bytes(&mut out, &self.target)?;
        write_bytes(&mut out, self.method.as_bytes())?;
        write_bytes(&mut out, &self.arg)?;
        out.push(self.status.to_u8());
        match self.reply.as_ref() {
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
        out.extend_from_slice(&self.updated_at.to_be_bytes());
        out.extend_from_slice(&self.call_started_at_time.to_be_bytes());
        let checksum = crc32_ieee(&out);
        out.extend_from_slice(&checksum.to_be_bytes());
        Some(out)
    }

    fn decode_checked(data: &[u8]) -> Option<Self> {
        let mut offset = 0usize;
        let version = *data.get(offset)?;
        offset += 1;
        if version != 2 {
            return None;
        }
        let request_id = TxId(read_array::<32>(data, &mut offset)?);
        let tx_id = TxId(read_array::<32>(data, &mut offset)?);
        let block_number = read_u64(data, &mut offset)?;
        let tx_index = read_u32(data, &mut offset)?;
        let log_index = read_u32(data, &mut offset)?;
        let tx_kind = TxKind::from_u8(*data.get(offset)?)?;
        offset += 1;
        let evm_sender = read_array::<20>(data, &mut offset)?;
        let ic_caller = match *data.get(offset)? {
            0 => {
                offset += 1;
                None
            }
            1 => {
                offset += 1;
                Some(read_bytes(data, &mut offset, MAX_TARGET_LEN, true)?)
            }
            _ => return None,
        };
        let target = read_bytes(data, &mut offset, MAX_TARGET_LEN, true)?;
        let method_bytes = read_bytes(data, &mut offset, MAX_METHOD_LEN, true)?;
        if !method_bytes.is_ascii() {
            return None;
        }
        let arg = read_bytes(data, &mut offset, MAX_ARG_LEN, false)?;
        let status = IcpUpdateRequestStatus::from_u8(*data.get(offset)?)?;
        offset += 1;
        let reply = match *data.get(offset)? {
            0 => {
                offset += 1;
                None
            }
            1 => {
                offset += 1;
                Some(read_bytes(data, &mut offset, MAX_RETURN_DATA, false)?)
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
                Some(String::from_utf8(read_bytes(data, &mut offset, MAX_ERROR_LEN, true)?).ok()?)
            }
            _ => return None,
        };
        let updated_at = read_u64(data, &mut offset)?;
        let call_started_at_time = read_u64(data, &mut offset)?;
        let remaining = data.len().checked_sub(offset)?;
        if remaining != CHECKSUM_LEN {
            return None;
        }
        let checksum_end = offset.checked_add(CHECKSUM_LEN)?;
        let checksum_raw = data.get(offset..checksum_end)?;
        let expected = u32::from_be_bytes(checksum_raw.try_into().ok()?);
        let actual = crc32_ieee(data.get(0..offset)?);
        if actual != expected {
            return None;
        }
        Some(Self {
            request_id,
            tx_id,
            block_number,
            tx_index,
            log_index,
            tx_kind,
            evm_sender,
            ic_caller,
            target,
            method: String::from_utf8(method_bytes).ok()?,
            arg,
            status,
            reply,
            error_code,
            updated_at,
            call_started_at_time,
        })
    }
}

fn read_array<const N: usize>(data: &[u8], offset: &mut usize) -> Option<[u8; N]> {
    let end = offset.checked_add(N)?;
    let raw = data.get(*offset..end)?;
    *offset = end;
    raw.try_into().ok()
}

fn read_u32(data: &[u8], offset: &mut usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let raw = data.get(*offset..end)?;
    *offset = end;
    Some(u32::from_be_bytes(raw.try_into().ok()?))
}

fn write_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Option<()> {
    let len = u16::try_from(bytes.len()).ok()?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Some(())
}

fn read_bytes(data: &[u8], offset: &mut usize, max_len: usize, non_empty: bool) -> Option<Vec<u8>> {
    let len_end = offset.checked_add(2)?;
    let len_raw = data.get(*offset..len_end)?;
    let len = u16::from_be_bytes([len_raw[0], len_raw[1]]) as usize;
    if len > max_len || (non_empty && len == 0) {
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

fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in data.iter().copied() {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB88320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::{IcpUpdateDispatchRequest, IcpUpdateRequestStatus, ICP_UPDATE_DECODE_FAILURE_CODE};
    use crate::chain_data::{TxId, TxKind};
    use ic_stable_structures::Storable;
    use std::borrow::Cow;

    #[test]
    fn icp_update_request_roundtrips() {
        let req = IcpUpdateDispatchRequest {
            request_id: TxId([9u8; 32]),
            tx_id: TxId([8u8; 32]),
            block_number: 12,
            tx_index: 1,
            log_index: 2,
            tx_kind: TxKind::IcSynthetic,
            evm_sender: [7u8; 20],
            ic_caller: Some(vec![6]),
            target: vec![1, 2, 3],
            method: "write_state".to_string(),
            arg: vec![4, 5],
            status: IcpUpdateRequestStatus::Dispatched,
            reply: Some(vec![6, 7]),
            error_code: None,
            updated_at: 11,
            call_started_at_time: 10,
        };

        let decoded = IcpUpdateDispatchRequest::from_bytes(Cow::Owned(req.to_bytes().into_owned()));
        assert_eq!(decoded, req);
    }

    #[test]
    fn icp_update_request_decode_failure_returns_failed_placeholder() {
        let decoded = IcpUpdateDispatchRequest::from_bytes(Cow::Owned(vec![0xffu8]));

        assert_eq!(decoded.status, IcpUpdateRequestStatus::DispatchFailed);
        assert_eq!(decoded.method, "decode_failure");
        assert_eq!(
            decoded.error_code,
            Some(ICP_UPDATE_DECODE_FAILURE_CODE.to_string())
        );
        assert_eq!(decoded.tx_kind, TxKind::EthSigned);
        assert_eq!(decoded.ic_caller, None);
    }
}
