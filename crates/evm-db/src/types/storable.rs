//! どこで: Stable構造体のStorable実装 / 何を: 固定長/可変長の境界指定 / なぜ: 安全なシリアライズのため

use crate::corrupt_log::record_corrupt;
use crate::decode::hash_to_array;
use crate::types::keys::{
    AccountKey, CodeKey, StorageKey, ACCOUNT_KEY_LEN, ACCOUNT_KEY_LEN_U32, STORAGE_KEY_LEN,
    STORAGE_KEY_LEN_U32,
};
use crate::types::values::{
    AccountVal, CodeVal, U256Val, ACCOUNT_VAL_LEN, ACCOUNT_VAL_LEN_U32, MAX_CODE_SIZE_U32,
    U256_LEN, U256_LEN_U32,
};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

impl Storable for AccountKey {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(&self.0)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != ACCOUNT_KEY_LEN {
            record_corrupt(b"account_key");
            return AccountKey(hash_to_array(b"account_key", data));
        }
        let mut buf = [0u8; ACCOUNT_KEY_LEN];
        buf.copy_from_slice(data);
        AccountKey(buf)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: ACCOUNT_KEY_LEN_U32,
        is_fixed_size: true,
    };
}

impl Storable for StorageKey {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(&self.0)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != STORAGE_KEY_LEN {
            record_corrupt(b"storage_key");
            return StorageKey(hash_to_array(b"storage_key", data));
        }
        let mut buf = [0u8; STORAGE_KEY_LEN];
        buf.copy_from_slice(data);
        StorageKey(buf)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: STORAGE_KEY_LEN_U32,
        is_fixed_size: true,
    };
}

impl Storable for CodeKey {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(&self.0)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 33 {
            record_corrupt(b"code_key");
            return CodeKey(hash_to_array(b"code_key", data));
        }
        let mut buf = [0u8; 33];
        buf.copy_from_slice(data);
        CodeKey(buf)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 33,
        is_fixed_size: true,
    };
}

impl Storable for AccountVal {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(&self.0)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != ACCOUNT_VAL_LEN {
            record_corrupt(b"account_val");
            return AccountVal([0u8; ACCOUNT_VAL_LEN]);
        }
        let mut buf = [0u8; ACCOUNT_VAL_LEN];
        buf.copy_from_slice(data);
        AccountVal(buf)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: ACCOUNT_VAL_LEN_U32,
        is_fixed_size: true,
    };
}

impl Storable for U256Val {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(&self.0)
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != U256_LEN {
            record_corrupt(b"u256_val");
            return U256Val([0u8; U256_LEN]);
        }
        let mut buf = [0u8; U256_LEN];
        buf.copy_from_slice(data);
        U256Val(buf)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: U256_LEN_U32,
        is_fixed_size: true,
    };
}

impl Storable for CodeVal {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(self.0.clone())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        CodeVal(bytes.to_vec())
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: MAX_CODE_SIZE_U32,
        is_fixed_size: false,
    };
}
