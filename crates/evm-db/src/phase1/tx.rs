//! どこで: Phase1のTxモデル / 何を: TxEnvelopeとID / なぜ: 決定性と互換性を固定するため

use crate::phase1::constants::{MAX_TX_SIZE, TX_ID_LEN, TX_ID_LEN_U32, MAX_TX_SIZE_U32};
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
pub struct TxEnvelope {
    pub tx_id: TxId,
    pub kind: TxKind,
    pub tx_bytes: Vec<u8>,
}

impl TxEnvelope {
    pub fn new(tx_id: TxId, kind: TxKind, tx_bytes: Vec<u8>) -> Self {
        Self {
            tx_id,
            kind,
            tx_bytes,
        }
    }
}

impl Storable for TxEnvelope {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = Vec::with_capacity(1 + TX_ID_LEN + 4 + self.tx_bytes.len());
        out.push(self.kind.to_u8());
        out.extend_from_slice(&self.tx_id.0);
        let len = len_to_u32(self.tx_bytes.len(), "tx_envelope: len overflow");
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&self.tx_bytes);
        Cow::Owned(out)
    }

    fn into_bytes(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(1 + TX_ID_LEN + 4 + self.tx_bytes.len());
        out.push(self.kind.to_u8());
        out.extend_from_slice(&self.tx_id.0);
        let len = len_to_u32(self.tx_bytes.len(), "tx_envelope: len overflow");
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&self.tx_bytes);
        out
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() < 1 + TX_ID_LEN + 4 {
            ic_cdk::trap("tx_envelope: invalid length");
        }
        let kind = TxKind::from_u8(data[0]).unwrap_or_else(|| ic_cdk::trap("tx_envelope: kind"));
        let mut tx_id = [0u8; TX_ID_LEN];
        tx_id.copy_from_slice(&data[1..1 + TX_ID_LEN]);
        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&data[1 + TX_ID_LEN..1 + TX_ID_LEN + 4]);
        let len = len_to_usize(u32::from_be_bytes(len_bytes), "tx_envelope: len overflow");
        let expected = 1 + TX_ID_LEN + 4 + len;
        if expected != data.len() {
            ic_cdk::trap("tx_envelope: length mismatch");
        }
        if len > MAX_TX_SIZE {
            ic_cdk::trap("tx_envelope: tx too large");
        }
        let tx_bytes = data[1 + TX_ID_LEN + 4..].to_vec();
        Self {
            tx_id: TxId(tx_id),
            kind,
            tx_bytes,
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 1 + TX_ID_LEN_U32 + 4 + MAX_TX_SIZE_U32,
        is_fixed_size: false,
    };
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

fn len_to_usize(len: u32, msg: &str) -> usize {
    usize::try_from(len).unwrap_or_else(|_| ic_cdk::trap(msg))
}
