//! どこで: chain_data の呼び出し元キー / 何を: Principal を固定長キーに変換 / なぜ: stable map のキーを固定長化するため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use crate::chain_data::constants::{CALLER_KEY_LEN, MAX_PRINCIPAL_LEN};
use crate::decode::hash_to_array;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct CallerKey(pub [u8; CALLER_KEY_LEN]);

impl CallerKey {
    pub fn from_principal_bytes(bytes: &[u8]) -> Self {
        if bytes.len() > MAX_PRINCIPAL_LEN {
            ic_cdk::trap("caller_key: principal too long");
        }
        let mut out = [0u8; CALLER_KEY_LEN];
        let len = u8::try_from(bytes.len()).unwrap_or_else(|_| ic_cdk::trap("caller_key: len"));
        out[0] = len;
        out[1..1 + bytes.len()].copy_from_slice(bytes);
        Self(out)
    }
}

impl Storable for CallerKey {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        encode_guarded(b"caller_key", self.0.to_vec(), CALLER_KEY_LEN as u32)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != CALLER_KEY_LEN {
            mark_decode_failure(b"caller_key", false);
            return CallerKey(hash_to_array(b"caller_key", data));
        }
        let mut out = [0u8; CALLER_KEY_LEN];
        out.copy_from_slice(data);
        Self(out)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: CALLER_KEY_LEN as u32,
        is_fixed_size: true,
    };
}
