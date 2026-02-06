//! どこで: Phase1のTxモデル / 何を: StoredTxBytesとID / なぜ: stableは生bytesを安全に保持するため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use crate::chain_data::constants::{
    MAX_PRINCIPAL_LEN, MAX_TX_SIZE, MAX_TX_SIZE_U32, TX_ID_LEN, TX_ID_LEN_U32,
};
use crate::corrupt_log::record_corrupt;
use crate::decode::hash_to_array;
use alloy_primitives::keccak256 as alloy_keccak256;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

const MAX_STORED_PRINCIPAL_LEN: usize = 64;
const STORED_TX_VERSION: u8 = 3;
const STORED_TX_MAX_SIZE_U32: u32 = 1
    + 1
    + TX_ID_LEN_U32
    + 1
    + 20
    + 16
    + 16
    + 2
    + MAX_PRINCIPAL_LEN as u32
    + 2
    + MAX_PRINCIPAL_LEN as u32
    + 4
    + MAX_TX_SIZE_U32;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct TxId(pub [u8; TX_ID_LEN]);

impl Storable for TxId {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        match encode_guarded(b"tx_id_encode", self.0.to_vec(), TX_ID_LEN_U32) {
            Ok(value) => value,
            Err(_) => Cow::Owned(vec![0u8; TX_ID_LEN_U32 as usize]),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != TX_ID_LEN {
            mark_decode_failure(b"tx_id", true);
            return TxId(hash_to_array(b"tx_id", data));
        }
        let mut buf = [0u8; TX_ID_LEN];
        buf.copy_from_slice(data);
        Self(buf)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: TX_ID_LEN_U32,
        is_fixed_size: true,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TxKind {
    EthSigned,
    IcSynthetic,
}

impl TxKind {
    pub fn to_u8(self) -> u8 {
        match self {
            TxKind::EthSigned => 0x01,
            TxKind::IcSynthetic => 0x02,
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(TxKind::EthSigned),
            0x02 => Some(TxKind::IcSynthetic),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredTxBytes {
    pub version: u8,
    pub tx_id: TxId,
    pub kind: TxKind,
    pub raw: Vec<u8>,
    pub caller_evm: Option<[u8; 20]>,
    pub canister_id: Vec<u8>,
    pub caller_principal: Vec<u8>,
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
    pub is_dynamic_fee: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoredTxBytesError {
    UnsupportedVersion(u8),
    InvalidLength,
    InvalidKind,
    DataTooLarge,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredTx {
    pub kind: TxKind,
    pub raw: Vec<u8>,
    pub caller_evm: Option<[u8; 20]>,
    pub canister_id: Vec<u8>,
    pub caller_principal: Vec<u8>,
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
    pub is_dynamic_fee: bool,
    pub tx_id: TxId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoredTxError {
    UnsupportedVersion(u8),
    EmptyBytes,
    MissingCaller,
    MissingPrincipal,
    CallerMismatch,
    TxIdMismatch,
}

impl StoredTxBytes {
    pub fn new_with_fees(
        tx_id: TxId,
        kind: TxKind,
        raw: Vec<u8>,
        caller_evm: Option<[u8; 20]>,
        canister_id: Vec<u8>,
        caller_principal: Vec<u8>,
        max_fee_per_gas: u128,
        max_priority_fee_per_gas: u128,
        is_dynamic_fee: bool,
    ) -> Self {
        Self {
            version: STORED_TX_VERSION,
            tx_id,
            kind,
            raw,
            caller_evm,
            canister_id,
            caller_principal,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            is_dynamic_fee,
        }
    }

    pub fn kind(&self) -> TxKind {
        self.kind
    }

    pub fn tx_id(&self) -> TxId {
        self.tx_id
    }

    pub fn raw(&self) -> &Vec<u8> {
        &self.raw
    }

    pub fn caller_evm(&self) -> Option<[u8; 20]> {
        self.caller_evm
    }

    pub fn fee_fields(&self) -> (u128, u128, bool) {
        (
            self.max_fee_per_gas,
            self.max_priority_fee_per_gas,
            self.is_dynamic_fee,
        )
    }

    pub fn is_invalid(&self) -> bool {
        self.raw.is_empty() || self.version != STORED_TX_VERSION
    }

    pub fn validate(&self) -> Result<(), StoredTxError> {
        if self.version != STORED_TX_VERSION {
            return Err(StoredTxError::UnsupportedVersion(self.version));
        }
        if self.raw.is_empty() {
            return Err(StoredTxError::EmptyBytes);
        }
        Ok(())
    }
}

impl TryFrom<StoredTxBytes> for StoredTx {
    type Error = StoredTxError;

    fn try_from(value: StoredTxBytes) -> Result<Self, Self::Error> {
        if value.version != STORED_TX_VERSION {
            return Err(StoredTxError::UnsupportedVersion(value.version));
        }
        if value.raw.is_empty() {
            return Err(StoredTxError::EmptyBytes);
        }
        if value.kind == TxKind::IcSynthetic && value.caller_evm.is_none() {
            return Err(StoredTxError::MissingCaller);
        }
        if value.kind == TxKind::IcSynthetic
            && (value.canister_id.is_empty() || value.caller_principal.is_empty())
        {
            return Err(StoredTxError::MissingPrincipal);
        }
        if value.kind != TxKind::IcSynthetic
            && (!value.canister_id.is_empty() || !value.caller_principal.is_empty())
        {
            return Err(StoredTxError::MissingPrincipal);
        }
        let expected = stored_tx_id(
            value.kind,
            &value.raw,
            value.caller_evm,
            if value.kind == TxKind::IcSynthetic {
                Some(value.canister_id.as_slice())
            } else {
                None
            },
            if value.kind == TxKind::IcSynthetic {
                Some(value.caller_principal.as_slice())
            } else {
                None
            },
        );
        if value.tx_id.0 != expected {
            return Err(StoredTxError::TxIdMismatch);
        }
        if value.kind == TxKind::IcSynthetic {
            let derived = caller_evm_from_principal(value.caller_principal.as_slice());
            if value.caller_evm != Some(derived) {
                return Err(StoredTxError::CallerMismatch);
            }
        }
        Ok(StoredTx {
            kind: value.kind,
            raw: value.raw,
            caller_evm: value.caller_evm,
            canister_id: value.canister_id,
            caller_principal: value.caller_principal,
            max_fee_per_gas: value.max_fee_per_gas,
            max_priority_fee_per_gas: value.max_priority_fee_per_gas,
            is_dynamic_fee: value.is_dynamic_fee,
            tx_id: value.tx_id,
        })
    }
}

impl Storable for StoredTxBytes {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        match encode_guarded(b"stored_tx_encode", encode(self), STORED_TX_MAX_SIZE_U32) {
            Ok(value) => value,
            Err(_) => Cow::Owned(encode_fallback_stored_tx()),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        encode(&self)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.is_empty() {
            return invalid_stored_tx(0, data);
        }
        match decode_result(data) {
            Ok(value) => value,
            Err(err) => {
                let version = data[0];
                invalid_stored_tx(version, err.raw)
            }
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: STORED_TX_MAX_SIZE_U32,
        is_fixed_size: false,
    };
}

struct DecodeFailure<'a> {
    raw: &'a [u8],
}

fn invalid_stored_tx(version: u8, raw: &[u8]) -> StoredTxBytes {
    mark_decode_failure(b"stored_tx_decode", true);
    // 旧形式や外部入力をtrapせず安全にrejectするための無効レコード。
    let tx_id = TxId(placeholder_hash(raw));
    StoredTxBytes {
        version,
        tx_id,
        kind: TxKind::EthSigned,
        raw: Vec::new(),
        caller_evm: None,
        canister_id: Vec::new(),
        caller_principal: Vec::new(),
        max_fee_per_gas: 0,
        max_priority_fee_per_gas: 0,
        is_dynamic_fee: false,
    }
}

fn placeholder_hash(raw: &[u8]) -> [u8; TX_ID_LEN] {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut out = [0u8; TX_ID_LEN];
    for (i, chunk) in out.chunks_exact_mut(8).enumerate() {
        let mut hash = FNV_OFFSET.wrapping_add(i as u64);
        for b in raw {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        chunk.copy_from_slice(&hash.to_be_bytes());
    }
    out
}

fn encode(inner: &StoredTxBytes) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        1 + 1
            + TX_ID_LEN
            + 1
            + 20
            + 16
            + 16
            + 2
            + inner.canister_id.len()
            + 2
            + inner.caller_principal.len()
            + 4
            + inner.raw.len(),
    );
    out.push(inner.version);
    out.push(inner.kind.to_u8());
    out.extend_from_slice(&inner.tx_id.0);
    let mut flags = 0u8;
    if inner.caller_evm.is_some() {
        flags |= 1 << 0;
    }
    if inner.is_dynamic_fee {
        flags |= 1 << 1;
    }
    out.push(flags);
    let caller = inner.caller_evm.unwrap_or([0u8; 20]);
    out.extend_from_slice(&caller);
    out.extend_from_slice(&inner.max_fee_per_gas.to_be_bytes());
    out.extend_from_slice(&inner.max_priority_fee_per_gas.to_be_bytes());
    let canister_len = match u16::try_from(inner.canister_id.len()) {
        Ok(value) => value,
        Err(_) => {
            record_corrupt(b"tx_envelope_canister_len");
            return encode_fallback_stored_tx();
        }
    };
    out.extend_from_slice(&canister_len.to_be_bytes());
    out.extend_from_slice(&inner.canister_id);
    let principal_len = match u16::try_from(inner.caller_principal.len()) {
        Ok(value) => value,
        Err(_) => {
            record_corrupt(b"tx_envelope_principal_len");
            return encode_fallback_stored_tx();
        }
    };
    out.extend_from_slice(&principal_len.to_be_bytes());
    out.extend_from_slice(&inner.caller_principal);
    let len = match len_to_u32(inner.raw.len()) {
        Some(value) => value,
        None => {
            return encode_fallback_stored_tx();
        }
    };
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&inner.raw);
    out
}

fn encode_fallback_stored_tx() -> Vec<u8> {
    let invalid = invalid_stored_tx(0, &[]);
    let encoded = encode_minimal_stored_tx(&invalid);
    match encode_guarded(b"stored_tx_encode", encoded, STORED_TX_MAX_SIZE_U32) {
        Ok(value) => value.into_owned(),
        // Fallback is intentionally invalid; empty bytes are forbidden.
        Err(_) => vec![STORED_TX_VERSION],
    }
}

fn encode_minimal_stored_tx(inner: &StoredTxBytes) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        1 + 1
            + TX_ID_LEN
            + 1
            + 20
            + 16
            + 16
            + 2
            + inner.canister_id.len()
            + 2
            + inner.caller_principal.len()
            + 4
            + inner.raw.len(),
    );
    out.push(inner.version);
    out.push(inner.kind.to_u8());
    out.extend_from_slice(&inner.tx_id.0);
    let mut flags = 0u8;
    if inner.caller_evm.is_some() {
        flags |= 1 << 0;
    }
    if inner.is_dynamic_fee {
        flags |= 1 << 1;
    }
    out.push(flags);
    let caller = inner.caller_evm.unwrap_or([0u8; 20]);
    out.extend_from_slice(&caller);
    out.extend_from_slice(&inner.max_fee_per_gas.to_be_bytes());
    out.extend_from_slice(&inner.max_priority_fee_per_gas.to_be_bytes());
    let canister_len = inner.canister_id.len() as u16;
    out.extend_from_slice(&canister_len.to_be_bytes());
    out.extend_from_slice(&inner.canister_id);
    let principal_len = inner.caller_principal.len() as u16;
    out.extend_from_slice(&principal_len.to_be_bytes());
    out.extend_from_slice(&inner.caller_principal);
    let len = inner.raw.len() as u32;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&inner.raw);
    out
}

fn decode_result(data: &[u8]) -> Result<StoredTxBytes, DecodeFailure<'_>> {
    if data.len() < 1 + 1 + TX_ID_LEN + 1 + 20 + 16 + 16 + 4 {
        return Err(DecodeFailure { raw: data });
    }
    let version = data[0];
    if version != STORED_TX_VERSION {
        return Err(DecodeFailure { raw: data });
    }
    let mut offset = 1;
    let kind = match TxKind::from_u8(data[offset]) {
        Some(value) => value,
        None => return Err(DecodeFailure { raw: data }),
    };
    offset += 1;
    let mut tx_id = [0u8; TX_ID_LEN];
    tx_id.copy_from_slice(&data[offset..offset + TX_ID_LEN]);
    offset += TX_ID_LEN;
    let flags = data[offset];
    offset += 1;
    let mut caller = [0u8; 20];
    caller.copy_from_slice(&data[offset..offset + 20]);
    offset += 20;
    let mut max_fee = [0u8; 16];
    max_fee.copy_from_slice(&data[offset..offset + 16]);
    offset += 16;
    let mut max_priority = [0u8; 16];
    max_priority.copy_from_slice(&data[offset..offset + 16]);
    offset += 16;
    let mut canister_len_bytes = [0u8; 2];
    canister_len_bytes.copy_from_slice(&data[offset..offset + 2]);
    offset += 2;
    let canister_len = u16::from_be_bytes(canister_len_bytes) as usize;
    let principal_limit = MAX_PRINCIPAL_LEN.min(MAX_STORED_PRINCIPAL_LEN);
    if canister_len > principal_limit {
        return Err(DecodeFailure { raw: data });
    }
    if data.len() < offset + canister_len + 2 {
        return Err(DecodeFailure { raw: data });
    }
    let canister_id = data[offset..offset + canister_len].to_vec();
    offset += canister_len;
    let mut principal_len_bytes = [0u8; 2];
    principal_len_bytes.copy_from_slice(&data[offset..offset + 2]);
    offset += 2;
    let principal_len = u16::from_be_bytes(principal_len_bytes) as usize;
    if principal_len > principal_limit {
        return Err(DecodeFailure { raw: data });
    }
    if data.len() < offset + principal_len + 4 {
        return Err(DecodeFailure { raw: data });
    }
    let caller_principal = data[offset..offset + principal_len].to_vec();
    offset += principal_len;
    let mut len_bytes = [0u8; 4];
    len_bytes.copy_from_slice(&data[offset..offset + 4]);
    offset += 4;
    let len: usize = match u32::from_be_bytes(len_bytes).try_into() {
        Ok(value) => value,
        Err(_) => return Err(DecodeFailure { raw: data }),
    };
    let expected = offset + len;
    if expected != data.len() {
        return Err(DecodeFailure { raw: data });
    }
    if len > MAX_TX_SIZE {
        return Err(DecodeFailure { raw: data });
    }
    let raw = data[offset..].to_vec();
    let caller_evm = if (flags & (1 << 0)) != 0 {
        Some(caller)
    } else {
        None
    };
    let is_dynamic_fee = (flags & (1 << 1)) != 0;
    Ok(StoredTxBytes {
        version,
        tx_id: TxId(tx_id),
        kind,
        raw,
        caller_evm,
        canister_id,
        caller_principal,
        max_fee_per_gas: u128::from_be_bytes(max_fee),
        max_priority_fee_per_gas: u128::from_be_bytes(max_priority),
        is_dynamic_fee,
    })
}

fn stored_tx_id(
    kind: TxKind,
    raw: &[u8],
    caller_evm: Option<[u8; 20]>,
    canister_id: Option<&[u8]>,
    caller_principal: Option<&[u8]>,
) -> [u8; TX_ID_LEN] {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"ic-evm:storedtx:v2");
    buf.push(kind.to_u8());
    buf.extend_from_slice(raw);
    if let Some(caller) = caller_evm {
        buf.extend_from_slice(&caller);
    }
    if let Some(bytes) = canister_id {
        let len = u16::try_from(bytes.len()).unwrap_or(0);
        buf.extend_from_slice(&len.to_be_bytes());
        buf.extend_from_slice(bytes);
    }
    if let Some(bytes) = caller_principal {
        let len = u16::try_from(bytes.len()).unwrap_or(0);
        buf.extend_from_slice(&len.to_be_bytes());
        buf.extend_from_slice(bytes);
    }
    alloy_keccak256(&buf).0
}

fn caller_evm_from_principal(principal_bytes: &[u8]) -> [u8; 20] {
    let mut payload = Vec::with_capacity("ic-evm:caller_evm:v1".len() + principal_bytes.len());
    payload.extend_from_slice(b"ic-evm:caller_evm:v1");
    payload.extend_from_slice(principal_bytes);
    let hash = alloy_keccak256(&payload).0;
    let mut out = [0u8; 20];
    out.copy_from_slice(&hash[12..32]);
    out
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TxIndexEntry {
    pub block_number: u64,
    pub tx_index: u32,
}

impl Storable for TxIndexEntry {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; 12];
        out[0..8].copy_from_slice(&self.block_number.to_be_bytes());
        out[8..12].copy_from_slice(&self.tx_index.to_be_bytes());
        match encode_guarded(b"tx_index_encode", out.to_vec(), 12) {
            Ok(value) => value,
            Err(_) => Cow::Owned(vec![0u8; 12]),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        let mut out = [0u8; 12];
        out[0..8].copy_from_slice(&self.block_number.to_be_bytes());
        out[8..12].copy_from_slice(&self.tx_index.to_be_bytes());
        out.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 12 {
            mark_decode_failure(b"tx_index", true);
            return TxIndexEntry {
                block_number: 0,
                tx_index: 0,
            };
        }
        let mut b = [0u8; 8];
        b.copy_from_slice(&data[0..8]);
        let mut t = [0u8; 4];
        t.copy_from_slice(&data[8..12]);
        Self {
            block_number: u64::from_be_bytes(b),
            tx_index: u32::from_be_bytes(t),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 12,
        is_fixed_size: true,
    };
}

fn len_to_u32(len: usize) -> Option<u32> {
    match u32::try_from(len) {
        Ok(value) => Some(value),
        Err(_) => {
            record_corrupt(b"tx_envelope_len");
            None
        }
    }
}
