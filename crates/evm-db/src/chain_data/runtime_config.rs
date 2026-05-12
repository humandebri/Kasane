//! どこで: unwrap 実行設定の永続層
//! 何を: wrap canister id と factory address を stable memory に保持
//! なぜ: install / upgrade 時の明示設定だけを正とし、実行時定数を排除するため

use crate::chain_data::codec::{encode_guarded, mark_decode_failure};
use candid::Principal;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;

pub const RUNTIME_CONFIG_SIZE_U32: u32 = 64;
const WRAP_CANISTER_MAX_BYTES: usize = 29;
const WRAP_FACTORY_ADDRESS_BYTES: usize = 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeConfigV1 {
    configured: bool,
    wrap_canister_len: u8,
    wrap_canister_bytes: [u8; WRAP_CANISTER_MAX_BYTES],
    wrap_factory_address: [u8; WRAP_FACTORY_ADDRESS_BYTES],
}

impl RuntimeConfigV1 {
    pub fn new_unconfigured() -> Self {
        Self {
            configured: false,
            wrap_canister_len: 0,
            wrap_canister_bytes: [0u8; WRAP_CANISTER_MAX_BYTES],
            wrap_factory_address: [0u8; WRAP_FACTORY_ADDRESS_BYTES],
        }
    }

    pub fn new(
        wrap_canister_id: Principal,
        wrap_factory_address: [u8; WRAP_FACTORY_ADDRESS_BYTES],
    ) -> Self {
        Self::try_new_from_bytes(wrap_canister_id.as_slice(), wrap_factory_address)
            .expect("principal length is valid")
    }

    pub fn new_from_bytes(
        wrap_canister_id: &[u8],
        wrap_factory_address: [u8; WRAP_FACTORY_ADDRESS_BYTES],
    ) -> Self {
        Self::try_new_from_bytes(wrap_canister_id, wrap_factory_address)
            .unwrap_or_else(|_| Self::new_unconfigured())
    }

    pub fn try_new_from_bytes(
        wrap_canister_id: &[u8],
        wrap_factory_address: [u8; WRAP_FACTORY_ADDRESS_BYTES],
    ) -> Result<Self, &'static str> {
        if !(1..=WRAP_CANISTER_MAX_BYTES).contains(&wrap_canister_id.len()) {
            return Err("runtime_config.wrap_canister_id_invalid");
        }
        let mut wrap_canister_bytes = [0u8; WRAP_CANISTER_MAX_BYTES];
        wrap_canister_bytes[..wrap_canister_id.len()].copy_from_slice(wrap_canister_id);
        Ok(Self {
            configured: true,
            wrap_canister_len: wrap_canister_id.len() as u8,
            wrap_canister_bytes,
            wrap_factory_address,
        })
    }

    pub fn wrap_canister_id(&self) -> Result<Principal, &'static str> {
        Ok(Principal::from_slice(&self.wrap_canister_id_bytes()?))
    }

    pub fn wrap_canister_id_bytes(&self) -> Result<Vec<u8>, &'static str> {
        if !self.configured {
            return Err("runtime_config.not_configured");
        }
        let len = usize::from(self.wrap_canister_len);
        if !(1..=WRAP_CANISTER_MAX_BYTES).contains(&len) {
            return Err("runtime_config.wrap_canister_id_invalid");
        }
        Ok(self.wrap_canister_bytes[..len].to_vec())
    }

    pub fn wrap_factory_address(&self) -> Result<[u8; WRAP_FACTORY_ADDRESS_BYTES], &'static str> {
        if !self.configured {
            return Err("runtime_config.not_configured");
        }
        Ok(self.wrap_factory_address)
    }
}

impl Default for RuntimeConfigV1 {
    fn default() -> Self {
        Self::new_unconfigured()
    }
}

impl Storable for RuntimeConfigV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; RUNTIME_CONFIG_SIZE_U32 as usize];
        out[0] = u8::from(self.configured);
        out[1] = self.wrap_canister_len;
        out[2..2 + WRAP_CANISTER_MAX_BYTES].copy_from_slice(&self.wrap_canister_bytes);
        out[32..32 + WRAP_FACTORY_ADDRESS_BYTES].copy_from_slice(&self.wrap_factory_address);
        match encode_guarded(
            b"runtime_config",
            Cow::Owned(out.to_vec()),
            RUNTIME_CONFIG_SIZE_U32,
        ) {
            Ok(value) => value,
            Err(_) => panic!("runtime_config: fixed-size encode failed"),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let data = bytes.as_ref();
        if data.len() != RUNTIME_CONFIG_SIZE_U32 as usize {
            mark_decode_failure(b"runtime_config", true);
            return Self::new_unconfigured();
        }

        let mut wrap_canister_bytes = [0u8; WRAP_CANISTER_MAX_BYTES];
        wrap_canister_bytes.copy_from_slice(&data[2..2 + WRAP_CANISTER_MAX_BYTES]);
        let mut wrap_factory_address = [0u8; WRAP_FACTORY_ADDRESS_BYTES];
        wrap_factory_address.copy_from_slice(&data[32..32 + WRAP_FACTORY_ADDRESS_BYTES]);
        Self {
            configured: data[0] != 0,
            wrap_canister_len: data[1],
            wrap_canister_bytes,
            wrap_factory_address,
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: RUNTIME_CONFIG_SIZE_U32,
        is_fixed_size: true,
    };
}
