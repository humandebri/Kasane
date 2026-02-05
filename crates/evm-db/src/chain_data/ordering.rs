//! どこで: Phase1.3の手数料順序 / 何を: ready_queue用キーとpendingキー / なぜ: 決定的な優先順とnonce待ちを両立するため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use crate::decode::hash_to_array;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

pub const READY_KEY_LEN: usize = 72;
pub const READY_KEY_LEN_U32: u32 = 72;
pub const SENDER_KEY_LEN: usize = 20;
pub const SENDER_KEY_LEN_U32: u32 = 20;
pub const SENDER_NONCE_KEY_LEN: usize = 28;
pub const SENDER_NONCE_KEY_LEN_U32: u32 = 28;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ReadyKey(pub [u8; READY_KEY_LEN]);

impl ReadyKey {
    pub fn new(
        max_fee_per_gas: u128,
        max_priority_fee_per_gas: u128,
        seq: u64,
        tx_hash: [u8; 32],
    ) -> Self {
        let max_fee_inv = u128::MAX.saturating_sub(max_fee_per_gas);
        let max_priority_inv = u128::MAX.saturating_sub(max_priority_fee_per_gas);
        let mut buf = [0u8; READY_KEY_LEN];
        buf[0..16].copy_from_slice(&max_fee_inv.to_be_bytes());
        buf[16..32].copy_from_slice(&max_priority_inv.to_be_bytes());
        buf[32..40].copy_from_slice(&seq.to_be_bytes());
        buf[40..72].copy_from_slice(&tx_hash);
        Self(buf)
    }

    pub fn seq(self) -> u64 {
        let mut raw = [0u8; 8];
        raw.copy_from_slice(&self.0[32..40]);
        u64::from_be_bytes(raw)
    }
}

impl Storable for ReadyKey {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        encode_guarded(b"ready_key", self.0.to_vec(), READY_KEY_LEN_U32)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        match data.len() {
            READY_KEY_LEN => {
                let mut buf = [0u8; READY_KEY_LEN];
                buf.copy_from_slice(data);
                Self(buf)
            }
            _ => {
                mark_decode_failure(b"ready_key", false);
                ReadyKey(hash_to_array(b"ready_key", data))
            }
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: READY_KEY_LEN_U32,
        is_fixed_size: true,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SenderKey(pub [u8; SENDER_KEY_LEN]);

impl SenderKey {
    pub fn new(sender: [u8; 20]) -> Self {
        Self(sender)
    }
}

impl Storable for SenderKey {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        encode_guarded(b"sender_key", self.0.to_vec(), SENDER_KEY_LEN_U32)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != SENDER_KEY_LEN {
            mark_decode_failure(b"sender_key", false);
            return SenderKey(hash_to_array(b"sender_key", data));
        }
        let mut buf = [0u8; SENDER_KEY_LEN];
        buf.copy_from_slice(data);
        Self(buf)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: SENDER_KEY_LEN_U32,
        is_fixed_size: true,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SenderNonceKey {
    pub sender: SenderKey,
    pub nonce: u64,
}

impl SenderNonceKey {
    pub fn new(sender: [u8; 20], nonce: u64) -> Self {
        Self {
            sender: SenderKey::new(sender),
            nonce,
        }
    }
}

impl Storable for SenderNonceKey {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; SENDER_NONCE_KEY_LEN];
        out[0..20].copy_from_slice(&self.sender.0);
        out[20..28].copy_from_slice(&self.nonce.to_be_bytes());
        encode_guarded(b"sender_nonce_key", out.to_vec(), SENDER_NONCE_KEY_LEN_U32)
    }

    fn into_bytes(self) -> Vec<u8> {
        let mut out = [0u8; SENDER_NONCE_KEY_LEN];
        out[0..20].copy_from_slice(&self.sender.0);
        out[20..28].copy_from_slice(&self.nonce.to_be_bytes());
        out.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != SENDER_NONCE_KEY_LEN {
            mark_decode_failure(b"sender_nonce_key", false);
            let hashed = hash_to_array::<SENDER_NONCE_KEY_LEN>(b"sender_nonce_key", data);
            let mut sender = [0u8; 20];
            sender.copy_from_slice(&hashed[0..20]);
            let mut nonce = [0u8; 8];
            nonce.copy_from_slice(&hashed[20..28]);
            return Self {
                sender: SenderKey(sender),
                nonce: u64::from_be_bytes(nonce),
            };
        }
        let mut sender = [0u8; 20];
        sender.copy_from_slice(&data[0..20]);
        let mut nonce = [0u8; 8];
        nonce.copy_from_slice(&data[20..28]);
        Self {
            sender: SenderKey(sender),
            nonce: u64::from_be_bytes(nonce),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: SENDER_NONCE_KEY_LEN_U32,
        is_fixed_size: true,
    };
}
