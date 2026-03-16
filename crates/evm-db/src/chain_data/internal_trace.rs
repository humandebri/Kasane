//! どこで: Phase1のinternal trace保存 / 何を: flatten済み内部アクションを保存 / なぜ: explorerでInternal Transactionsを出すため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use crate::chain_data::constants::{MAX_TXS_PER_BLOCK_U32, RECEIPT_CONTRACT_ADDR_LEN};
use crate::corrupt_log::record_corrupt;
use crate::decode::{read_array, read_exact, read_u16, read_u32, read_u64, read_u8, read_vec};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

pub const MAX_INTERNAL_TRACES_PER_TX_U32: u32 = MAX_TXS_PER_BLOCK_U32;
const MAX_TRACE_ID_LEN_U16: u16 = 64;
const MAX_ERROR_CODE_LEN_U16: u16 = 96;
const INTERNAL_TRACES_MAGIC: [u8; 8] = *b"itrace01";
const INTERNAL_TRACES_MAX_SIZE_U32: u32 = 4
    + 8
    + 1
    + 1
    + 4
    + 4
    + 4
    + MAX_INTERNAL_TRACES_PER_TX_U32
        * (8 + 4
            + 2
            + MAX_TRACE_ID_LEN_U16 as u32
            + 2
            + 1
            + 20
            + 1
            + 20
            + 32
            + 1
            + 20
            + 1
            + 2
            + MAX_ERROR_CODE_LEN_U16 as u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InternalTraceActionKind {
    Call,
    CallCode,
    DelegateCall,
    StaticCall,
    Create,
    Create2,
    Custom,
    Selfdestruct,
}

impl InternalTraceActionKind {
    pub fn to_u8(self) -> u8 {
        match self {
            Self::Call => 1,
            Self::CallCode => 2,
            Self::DelegateCall => 3,
            Self::StaticCall => 4,
            Self::Create => 5,
            Self::Create2 => 6,
            Self::Custom => 7,
            Self::Selfdestruct => 8,
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Call),
            2 => Some(Self::CallCode),
            3 => Some(Self::DelegateCall),
            4 => Some(Self::StaticCall),
            5 => Some(Self::Create),
            6 => Some(Self::Create2),
            7 => Some(Self::Custom),
            8 => Some(Self::Selfdestruct),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalTrace {
    pub block_number: u64,
    pub tx_index: u32,
    pub trace_id: String,
    pub depth: u16,
    pub action_kind: InternalTraceActionKind,
    pub from_address: [u8; RECEIPT_CONTRACT_ADDR_LEN],
    pub to_address: Option<[u8; RECEIPT_CONTRACT_ADDR_LEN]>,
    pub value: [u8; 32],
    pub created_contract_address: Option<[u8; RECEIPT_CONTRACT_ADDR_LEN]>,
    pub success: bool,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalTraceSet {
    pub items: Vec<InternalTrace>,
    pub encode_failed: bool,
    pub captured_count: u32,
    pub total_count: u32,
    pub truncated: bool,
}

impl InternalTraceSet {
    pub fn new(items: Vec<InternalTrace>) -> Self {
        let captured_count = u32::try_from(items.len()).unwrap_or(u32::MAX);
        Self {
            items,
            encode_failed: false,
            captured_count,
            total_count: captured_count,
            truncated: false,
        }
    }

    pub fn new_with_counts(items: Vec<InternalTrace>, total_count: u32) -> Self {
        let captured_count = u32::try_from(items.len()).unwrap_or(u32::MAX);
        Self {
            items,
            encode_failed: false,
            captured_count,
            total_count,
            truncated: total_count > captured_count,
        }
    }

    pub fn failed(total_count: u32) -> Self {
        Self {
            items: Vec::new(),
            encode_failed: true,
            captured_count: 0,
            total_count,
            truncated: false,
        }
    }

    pub fn to_bytes_checked(&self) -> Result<Cow<'_, [u8]>, InternalTraceEncodeError> {
        let encoded = self.encode_checked()?;
        encode_guarded(
            b"internal_traces",
            Cow::Owned(encoded),
            INTERNAL_TRACES_MAX_SIZE_U32,
        )
        .map_err(|_| InternalTraceEncodeError::EncodedSizeExceeded)
    }

    fn encode_checked(&self) -> Result<Vec<u8>, InternalTraceEncodeError> {
        let count =
            u32::try_from(self.items.len()).map_err(|_| InternalTraceEncodeError::TooManyItems)?;
        if count > MAX_INTERNAL_TRACES_PER_TX_U32
            || self.captured_count > MAX_INTERNAL_TRACES_PER_TX_U32
            || self.captured_count != count
            || self.total_count < self.captured_count
            || (!self.encode_failed && self.truncated != (self.total_count > self.captured_count))
            || (self.encode_failed
                && (self.captured_count != 0 || !self.items.is_empty() || self.truncated))
        {
            record_corrupt(b"internal_traces_len");
            return Err(InternalTraceEncodeError::TooManyItems);
        }
        let mut out = Vec::new();
        out.extend_from_slice(&INTERNAL_TRACES_MAGIC);
        out.push(u8::from(self.encode_failed));
        out.push(u8::from(self.truncated));
        out.extend_from_slice(&self.captured_count.to_be_bytes());
        out.extend_from_slice(&self.total_count.to_be_bytes());
        out.extend_from_slice(&count.to_be_bytes());
        for item in self.items.iter() {
            let trace_id = item.trace_id.as_bytes();
            let trace_id_len = u16::try_from(trace_id.len())
                .map_err(|_| InternalTraceEncodeError::TraceIdTooLong)?;
            if trace_id_len > MAX_TRACE_ID_LEN_U16 {
                record_corrupt(b"internal_traces_trace_id");
                return Err(InternalTraceEncodeError::TraceIdTooLong);
            }
            let error_code_bytes = item
                .error_code
                .as_ref()
                .map(|value| value.as_bytes())
                .unwrap_or(&[]);
            let error_len = u16::try_from(error_code_bytes.len())
                .map_err(|_| InternalTraceEncodeError::ErrorCodeTooLong)?;
            if error_len > MAX_ERROR_CODE_LEN_U16 {
                record_corrupt(b"internal_traces_error_code");
                return Err(InternalTraceEncodeError::ErrorCodeTooLong);
            }
            out.extend_from_slice(&item.block_number.to_be_bytes());
            out.extend_from_slice(&item.tx_index.to_be_bytes());
            out.extend_from_slice(&trace_id_len.to_be_bytes());
            out.extend_from_slice(trace_id);
            out.extend_from_slice(&item.depth.to_be_bytes());
            out.push(item.action_kind.to_u8());
            out.extend_from_slice(&item.from_address);
            match item.to_address {
                Some(value) => {
                    out.push(1);
                    out.extend_from_slice(&value);
                }
                None => out.push(0),
            }
            out.extend_from_slice(&item.value);
            match item.created_contract_address {
                Some(value) => {
                    out.push(1);
                    out.extend_from_slice(&value);
                }
                None => out.push(0),
            }
            out.push(u8::from(item.success));
            out.extend_from_slice(&error_len.to_be_bytes());
            out.extend_from_slice(error_code_bytes);
        }
        Ok(out)
    }
}

impl Storable for InternalTraceSet {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        self.to_bytes_checked().unwrap_or_else(|err| {
            record_corrupt(b"internal_traces_encode");
            encode_failed_internal_traces(self.total_count).unwrap_or_else(|| {
                panic!("internal_traces failed marker must encode after {err:?}")
            })
        })
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() > INTERNAL_TRACES_MAX_SIZE_U32 as usize {
            mark_decode_failure(b"internal_traces", false);
            return Self::new(Vec::new());
        }
        let mut offset = 0usize;
        let Some(magic) = read_exact(data, &mut offset, INTERNAL_TRACES_MAGIC.len()) else {
            mark_decode_failure(b"internal_traces", false);
            return Self::new(Vec::new());
        };
        if magic != INTERNAL_TRACES_MAGIC {
            mark_decode_failure(b"internal_traces", false);
            return Self::new(Vec::new());
        }
        let Some(encode_failed_raw) = read_u8(data, &mut offset) else {
            mark_decode_failure(b"internal_traces", false);
            return Self::new(Vec::new());
        };
        if encode_failed_raw > 1 {
            mark_decode_failure(b"internal_traces", false);
            return Self::new(Vec::new());
        }
        let Some(truncated_raw) = read_u8(data, &mut offset) else {
            mark_decode_failure(b"internal_traces", false);
            return Self::new(Vec::new());
        };
        if truncated_raw > 1 {
            mark_decode_failure(b"internal_traces", false);
            return Self::new(Vec::new());
        }
        let encode_failed = encode_failed_raw == 1;
        let Some(captured_count) = read_u32(data, &mut offset) else {
            mark_decode_failure(b"internal_traces", false);
            return Self::new(Vec::new());
        };
        let Some(total_count) = read_u32(data, &mut offset) else {
            mark_decode_failure(b"internal_traces", false);
            return Self::new(Vec::new());
        };
        let Some(count) = read_u32(data, &mut offset) else {
            mark_decode_failure(b"internal_traces", false);
            return Self::new(Vec::new());
        };
        let truncated = truncated_raw == 1;
        if count > MAX_INTERNAL_TRACES_PER_TX_U32
            || captured_count != count
            || total_count < captured_count
            || (!encode_failed && truncated != (total_count > captured_count))
            || (encode_failed && (captured_count != 0 || count != 0 || truncated))
        {
            mark_decode_failure(b"internal_traces", false);
            return Self::new(Vec::new());
        }
        let mut items = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let Some(block_number) = read_u64(data, &mut offset) else {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            };
            let Some(tx_index) = read_u32(data, &mut offset) else {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            };
            let Some(trace_id_len) = read_u16(data, &mut offset) else {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            };
            if trace_id_len > MAX_TRACE_ID_LEN_U16 {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            }
            let Some(trace_id_bytes) = read_vec(data, &mut offset, usize::from(trace_id_len))
            else {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            };
            let Ok(trace_id) = String::from_utf8(trace_id_bytes) else {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            };
            let Some(depth) = read_u16(data, &mut offset) else {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            };
            let Some(action_raw) = read_u8(data, &mut offset) else {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            };
            let Some(action_kind) = InternalTraceActionKind::from_u8(action_raw) else {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            };
            let Some(from_address) = read_array::<RECEIPT_CONTRACT_ADDR_LEN>(data, &mut offset)
            else {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            };
            let Some(has_to) = read_u8(data, &mut offset) else {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            };
            let to_address = if has_to == 0 {
                None
            } else {
                let Some(value) = read_array::<RECEIPT_CONTRACT_ADDR_LEN>(data, &mut offset) else {
                    mark_decode_failure(b"internal_traces", false);
                    return Self::new(Vec::new());
                };
                Some(value)
            };
            let Some(value) = read_array::<32>(data, &mut offset) else {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            };
            let Some(has_created) = read_u8(data, &mut offset) else {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            };
            let created_contract_address = if has_created == 0 {
                None
            } else {
                let Some(value) = read_array::<RECEIPT_CONTRACT_ADDR_LEN>(data, &mut offset) else {
                    mark_decode_failure(b"internal_traces", false);
                    return Self::new(Vec::new());
                };
                Some(value)
            };
            let Some(success) = read_u8(data, &mut offset) else {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            };
            if success > 1 {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            }
            let Some(error_len) = read_u16(data, &mut offset) else {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            };
            if error_len > MAX_ERROR_CODE_LEN_U16 {
                mark_decode_failure(b"internal_traces", false);
                return Self::new(Vec::new());
            }
            let error_code = if error_len == 0 {
                None
            } else {
                let Some(error_bytes) = read_vec(data, &mut offset, usize::from(error_len)) else {
                    mark_decode_failure(b"internal_traces", false);
                    return Self::new(Vec::new());
                };
                let Ok(error_code) = String::from_utf8(error_bytes) else {
                    mark_decode_failure(b"internal_traces", false);
                    return Self::new(Vec::new());
                };
                Some(error_code)
            };
            items.push(InternalTrace {
                block_number,
                tx_index,
                trace_id,
                depth,
                action_kind,
                from_address,
                to_address,
                value,
                created_contract_address,
                success: success == 1,
                error_code,
            });
        }
        Self {
            items,
            encode_failed,
            captured_count,
            total_count,
            truncated,
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: INTERNAL_TRACES_MAX_SIZE_U32,
        is_fixed_size: false,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InternalTraceEncodeError {
    TooManyItems,
    TraceIdTooLong,
    ErrorCodeTooLong,
    EncodedSizeExceeded,
}

fn encode_failed_internal_traces(total_count: u32) -> Option<Cow<'static, [u8]>> {
    InternalTraceSet::failed(total_count)
        .to_bytes_checked()
        .ok()
        .map(|value| Cow::Owned(value.into_owned()))
}
