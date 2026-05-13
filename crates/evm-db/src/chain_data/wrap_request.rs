//! どこで: 統合wrap状態 / 何を: stable保存型 / なぜ: gateway内でwrap要求を保持するため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use candid::{CandidType, Deserialize};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

pub const PRINCIPAL_MAX_BYTES: usize = 29;
pub const WRAP_STORED_REQUEST_MAX_BYTES: u32 = 2_048;
pub const FEE_POLICY_MAX_BYTES: u32 = 128;
pub const WRAP_EVM_CONFIG_MAX_BYTES: u32 = 32;
pub const WRAP_PENDING_SUBMISSION_MAX_BYTES: u32 = 96;
pub const WRAP_DECODE_FAILURE_CODE: &str = "stable.decode.wrap_request";

const WRAP_REQUEST_MAGIC: &[u8; 4] = b"KWR1";
const CHECKSUM_LEN: usize = 4;

#[derive(Clone, Copy, Debug, CandidType, Default, Deserialize, Eq, PartialEq)]
pub enum WrapRequestStage {
    FeePending,
    #[default]
    FeeCollected,
    PullPending,
    Pulled,
    MintSubmitting,
    MintSubmitted,
    Succeeded,
    Failed,
    Refunding,
    Refunded,
}

#[derive(Clone, Copy, Debug, CandidType, Default, Deserialize, Eq, PartialEq)]
pub enum MintSubmitStatus {
    #[default]
    NotSubmitted,
    Submitting,
    Submitted,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub enum RequestStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct WrapRequestResult {
    pub status: RequestStatus,
    pub pull_ledger_tx_id: Option<Vec<u8>>,
    pub mint_tx_id: Option<Vec<u8>>,
    pub error_code: Option<String>,
    #[serde(default)]
    pub withdrawn: bool,
    #[serde(default)]
    pub withdraw_ledger_tx_id: Option<Vec<u8>>,
    #[serde(default)]
    pub withdraw_error_code: Option<String>,
    #[serde(default)]
    pub withdraw_in_progress: bool,
    #[serde(default)]
    pub mint_failed_recoverable: bool,
    #[serde(default)]
    pub fee_ledger_tx_id: Option<Vec<u8>>,
    #[serde(default)]
    pub charged_fee_e8s: Option<u128>,
    #[serde(default)]
    pub charged_gas_price_wei: Option<u128>,
    #[serde(default)]
    pub stage: WrapRequestStage,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub mint_nonce: Option<u64>,
    #[serde(default)]
    pub mint_submitted_at_time: u64,
    #[serde(default)]
    pub mint_submit_status: MintSubmitStatus,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct WrapStoredRequest {
    pub caller: Vec<u8>,
    pub asset_id: Vec<u8>,
    pub amount: Vec<u8>,
    pub evm_recipient: Vec<u8>,
    pub gas_limit: u64,
    #[serde(default)]
    pub fee_ledger_canister: Vec<u8>,
    #[serde(default)]
    pub max_fee_e8s: u128,
    #[serde(default)]
    pub quoted_gas_price_wei: u128,
    #[serde(default)]
    pub fee_created_at_time: u64,
    #[serde(default)]
    pub pull_created_at_time: u64,
    #[serde(default)]
    pub withdraw_created_at_time: u64,
    pub result: WrapRequestResult,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct FeePolicyStored {
    pub fee_ledger_canister: Vec<u8>,
    pub cycle_fee_e8s: u64,
    pub gas_price_buffer_bps: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct WrapEvmConfigStored {
    pub wrap_factory_address: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct WrapPendingSubmission {
    pub caller: Vec<u8>,
    pub request_id: Vec<u8>,
}

impl Storable for WrapStoredRequest {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let payload = candid::encode_one(self).expect("wrap_stored_request.encode_failed");
        let mut out = Vec::with_capacity(WRAP_REQUEST_MAGIC.len() + payload.len() + CHECKSUM_LEN);
        out.extend_from_slice(WRAP_REQUEST_MAGIC);
        out.extend_from_slice(&payload);
        let checksum = crc32_ieee(&out);
        out.extend_from_slice(&checksum.to_be_bytes());
        encode_guarded(
            b"wrap_stored_request",
            Cow::Owned(out),
            WRAP_STORED_REQUEST_MAX_BYTES,
        )
        .unwrap_or_else(|_| panic!("wrap_stored_request.encode_guard_failed"))
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        decode_wrap_stored_request(bytes.as_ref()).unwrap_or_else(|| {
            mark_decode_failure(b"wrap_stored_request", false);
            WrapStoredRequest::decode_failure_placeholder()
        })
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: WRAP_STORED_REQUEST_MAX_BYTES,
        is_fixed_size: false,
    };
}

impl Storable for FeePolicyStored {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(candid::encode_one(self).expect("fee_policy.encode_failed"))
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        candid::decode_one::<Self>(bytes.as_ref()).unwrap_or_else(|_| {
            mark_decode_failure(b"fee_policy", false);
            Self::decode_failure_placeholder()
        })
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: FEE_POLICY_MAX_BYTES,
        is_fixed_size: false,
    };
}

impl Storable for WrapEvmConfigStored {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(candid::encode_one(self).expect("wrap_evm_config.encode_failed"))
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        candid::decode_one::<Self>(bytes.as_ref()).unwrap_or_else(|_| {
            mark_decode_failure(b"wrap_evm_config", false);
            Self::decode_failure_placeholder()
        })
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: WRAP_EVM_CONFIG_MAX_BYTES,
        is_fixed_size: false,
    };
}

impl Storable for WrapPendingSubmission {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(candid::encode_one(self).expect("wrap_pending_submission.encode_failed"))
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        candid::decode_one::<Self>(bytes.as_ref()).unwrap_or_else(|_| {
            mark_decode_failure(b"wrap_pending_submission", false);
            Self::decode_failure_placeholder()
        })
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: WRAP_PENDING_SUBMISSION_MAX_BYTES,
        is_fixed_size: false,
    };
}

impl WrapStoredRequest {
    fn decode_failure_placeholder() -> Self {
        Self {
            caller: vec![0u8],
            asset_id: vec![0u8],
            amount: vec![0u8; 32],
            evm_recipient: vec![0u8; 20],
            gas_limit: 1,
            fee_ledger_canister: vec![0u8],
            max_fee_e8s: 0,
            quoted_gas_price_wei: 0,
            fee_created_at_time: 0,
            pull_created_at_time: 0,
            withdraw_created_at_time: 0,
            result: WrapRequestResult {
                status: RequestStatus::Failed,
                pull_ledger_tx_id: None,
                mint_tx_id: None,
                error_code: Some(WRAP_DECODE_FAILURE_CODE.to_string()),
                withdrawn: false,
                withdraw_ledger_tx_id: None,
                withdraw_error_code: None,
                withdraw_in_progress: false,
                mint_failed_recoverable: false,
                fee_ledger_tx_id: None,
                charged_fee_e8s: Some(0),
                charged_gas_price_wei: Some(0),
                stage: WrapRequestStage::Failed,
                updated_at: 0,
                mint_nonce: None,
                mint_submitted_at_time: 0,
                mint_submit_status: MintSubmitStatus::NotSubmitted,
            },
        }
    }
}

impl FeePolicyStored {
    fn decode_failure_placeholder() -> Self {
        Self {
            fee_ledger_canister: Vec::new(),
            cycle_fee_e8s: 0,
            gas_price_buffer_bps: 0,
        }
    }
}

impl WrapEvmConfigStored {
    fn decode_failure_placeholder() -> Self {
        Self {
            wrap_factory_address: Vec::new(),
        }
    }
}

impl WrapPendingSubmission {
    fn decode_failure_placeholder() -> Self {
        Self {
            caller: Vec::new(),
            request_id: Vec::new(),
        }
    }

    pub fn is_decode_failure_placeholder(&self) -> bool {
        self.caller.is_empty() || self.request_id.len() != 32
    }
}

fn decode_wrap_stored_request(data: &[u8]) -> Option<WrapStoredRequest> {
    if !data.starts_with(WRAP_REQUEST_MAGIC) {
        return candid::decode_one::<WrapStoredRequest>(data).ok();
    }
    let payload_start = WRAP_REQUEST_MAGIC.len();
    let checksum_start = data.len().checked_sub(CHECKSUM_LEN)?;
    if checksum_start <= payload_start {
        return None;
    }
    let checksum_raw = data.get(checksum_start..)?;
    let expected = u32::from_be_bytes(checksum_raw.try_into().ok()?);
    let actual = crc32_ieee(data.get(..checksum_start)?);
    if actual != expected {
        return None;
    }
    candid::decode_one::<WrapStoredRequest>(data.get(payload_start..checksum_start)?).ok()
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
