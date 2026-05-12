//! どこで: native ICP deposit credit記録
//! 何を: request単位の冪等化情報
//! なぜ: ledger pull後のcredit再試行で二重mintを防ぐため

use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

const RECIPIENT_LEN: usize = 20;
const AMOUNT_LEN: usize = 32;
const ENCODED_LEN: usize = RECIPIENT_LEN + AMOUNT_LEN;
pub const NATIVE_CREDIT_RECORD_SIZE_U32: u32 = ENCODED_LEN as u32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeCreditRecord {
    pub recipient: [u8; RECIPIENT_LEN],
    pub amount_wei: [u8; AMOUNT_LEN],
}

impl NativeCreditRecord {
    pub fn new(recipient: [u8; RECIPIENT_LEN], amount_wei: [u8; AMOUNT_LEN]) -> Self {
        Self {
            recipient,
            amount_wei,
        }
    }
}

impl Storable for NativeCreditRecord {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; ENCODED_LEN];
        out[..RECIPIENT_LEN].copy_from_slice(&self.recipient);
        out[RECIPIENT_LEN..].copy_from_slice(&self.amount_wei);
        Cow::Owned(out.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let raw = bytes.as_ref();
        if raw.len() != ENCODED_LEN {
            ic_cdk::trap("native_credit.decode_failed");
        }
        let mut recipient = [0u8; RECIPIENT_LEN];
        recipient.copy_from_slice(&raw[..RECIPIENT_LEN]);
        let mut amount_wei = [0u8; AMOUNT_LEN];
        amount_wei.copy_from_slice(&raw[RECIPIENT_LEN..]);
        Self {
            recipient,
            amount_wei,
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: NATIVE_CREDIT_RECORD_SIZE_U32,
        is_fixed_size: true,
    };
}
