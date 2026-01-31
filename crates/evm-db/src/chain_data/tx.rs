//! どこで: Phase1のTxモデル / 何を: StoredTxとID / なぜ: stableは生bytesを安全に保持するため

use crate::chain_data::constants::{MAX_TX_SIZE, TX_ID_LEN, TX_ID_LEN_U32, MAX_TX_SIZE_U32};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct TxId(pub [u8; TX_ID_LEN]);

impl Storable for TxId {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(self.0.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != TX_ID_LEN {
            ic_cdk::trap("tx_id: invalid length");
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

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TxKind {
    EthSigned = 0,
    IcSynthetic = 1,
}

impl TxKind {
    pub fn to_u8(self) -> u8 {
        match self {
            TxKind::EthSigned => 0,
            TxKind::IcSynthetic => 1,
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(TxKind::EthSigned),
            1 => Some(TxKind::IcSynthetic),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredTx {
    pub version: u8,
    pub tx_id: TxId,
    pub kind: TxKind,
    pub raw: Vec<u8>,
    pub caller_evm: Option<[u8; 20]>,
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
    pub is_dynamic_fee: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoredTxError {
    UnsupportedVersion(u8),
    EmptyBytes,
}

impl StoredTx {
    pub fn new_with_fees(
        tx_id: TxId,
        kind: TxKind,
        raw: Vec<u8>,
        caller_evm: Option<[u8; 20]>,
        max_fee_per_gas: u128,
        max_priority_fee_per_gas: u128,
        is_dynamic_fee: bool,
    ) -> Self {
        Self {
            version: 2,
            tx_id,
            kind,
            raw,
            caller_evm,
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
        self.raw.is_empty() || self.version != 2
    }

    pub fn validate(&self) -> Result<(), StoredTxError> {
        if self.version != 2 {
            return Err(StoredTxError::UnsupportedVersion(self.version));
        }
        if self.raw.is_empty() {
            return Err(StoredTxError::EmptyBytes);
        }
        Ok(())
    }
}

impl Storable for StoredTx {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(encode(self))
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
        max_size: 1 + 1 + TX_ID_LEN_U32 + 1 + 20 + 16 + 16 + 4 + MAX_TX_SIZE_U32,
        is_fixed_size: false,
    };
}

struct DecodeFailure<'a> {
    raw: &'a [u8],
}

fn invalid_stored_tx(version: u8, raw: &[u8]) -> StoredTx {
    // 旧形式や外部入力をtrapせず安全にrejectするための無効レコード。
    let tx_id = TxId(placeholder_hash(raw));
    StoredTx {
        version,
        tx_id,
        kind: TxKind::EthSigned,
        raw: Vec::new(),
        caller_evm: None,
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

fn encode(inner: &StoredTx) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + 1 + TX_ID_LEN + 1 + 20 + 16 + 16 + 4 + inner.raw.len());
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
    let len = len_to_u32(inner.raw.len(), "tx_envelope: len overflow");
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&inner.raw);
    out
}

fn decode_result(data: &[u8]) -> Result<StoredTx, DecodeFailure<'_>> {
    if data.len() < 1 + 1 + TX_ID_LEN + 1 + 20 + 16 + 16 + 4 {
        return Err(DecodeFailure { raw: data });
    }
    let version = data[0];
    if version != 2 {
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
    let caller_evm = if (flags & (1 << 0)) != 0 { Some(caller) } else { None };
    let is_dynamic_fee = (flags & (1 << 1)) != 0;
    Ok(StoredTx {
        version,
        tx_id: TxId(tx_id),
        kind,
        raw,
        caller_evm,
        max_fee_per_gas: u128::from_be_bytes(max_fee),
        max_priority_fee_per_gas: u128::from_be_bytes(max_priority),
        is_dynamic_fee,
    })
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
        Cow::Owned(out.to_vec())
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
            ic_cdk::trap("tx_index: invalid length");
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

fn len_to_u32(len: usize, msg: &str) -> u32 {
    u32::try_from(len).unwrap_or_else(|_| ic_cdk::trap(msg))
}
