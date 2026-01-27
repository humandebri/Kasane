//! どこで: Stable構造体のStorable実装 / 何を: 固定長/可変長の境界指定 / なぜ: 安全なシリアライズのため

use crate::types::keys::{AccountKey, CodeKey, StorageKey};
use crate::types::values::{
    AccountVal, CodeVal, U256Val, ACCOUNT_VAL_LEN, ACCOUNT_VAL_LEN_U32, MAX_CODE_SIZE_U32, U256_LEN,
    U256_LEN_U32,
};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

impl Storable for AccountKey {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(self.0.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 21 {
            ic_cdk::trap("account_key: invalid length");
        }
        let mut buf = [0u8; 21];
        buf.copy_from_slice(data);
        AccountKey(buf)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 21,
        is_fixed_size: true,
    };
}

impl Storable for StorageKey {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(self.0.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 53 {
            ic_cdk::trap("storage_key: invalid length");
        }
        let mut buf = [0u8; 53];
        buf.copy_from_slice(data);
        StorageKey(buf)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 53,
        is_fixed_size: true,
    };
}

impl Storable for CodeKey {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(self.0.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != 33 {
            ic_cdk::trap("code_key: invalid length");
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
        Cow::Owned(self.0.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != ACCOUNT_VAL_LEN {
            ic_cdk::trap("account_val: invalid length");
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
        Cow::Owned(self.0.to_vec())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.0.to_vec()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != U256_LEN {
            ic_cdk::trap("u256_val: invalid length");
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
