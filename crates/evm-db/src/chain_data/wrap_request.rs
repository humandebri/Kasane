//! どこで: 統合wrap状態 / 何を: stable保存型 / なぜ: gateway内でwrap要求を保持するため

use candid::{CandidType, Deserialize};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

pub const PRINCIPAL_MAX_BYTES: usize = 29;
pub const WRAP_STORED_REQUEST_MAX_BYTES: u32 = 768;
pub const FEE_POLICY_MAX_BYTES: u32 = 128;
pub const WRAP_EVM_CONFIG_MAX_BYTES: u32 = 32;
pub const WRAP_PENDING_SUBMISSION_MAX_BYTES: u32 = 96;

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
        Cow::Owned(candid::encode_one(self).expect("wrap_stored_request.encode_failed"))
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        candid::decode_one::<Self>(bytes.as_ref()).expect("wrap_stored_request.decode_failed")
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
        candid::decode_one::<Self>(bytes.as_ref()).expect("fee_policy.decode_failed")
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
        candid::decode_one::<Self>(bytes.as_ref()).expect("wrap_evm_config.decode_failed")
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
        candid::decode_one::<Self>(bytes.as_ref()).expect("wrap_pending_submission.decode_failed")
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: WRAP_PENDING_SUBMISSION_MAX_BYTES,
        is_fixed_size: false,
    };
}
