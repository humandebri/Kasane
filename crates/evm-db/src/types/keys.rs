//! どこで: StableBTreeMapのKey / 何を: 固定長キー定義 / なぜ: 決定的な順序を保証するため

use std::mem::size_of;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct AccountKey(pub [u8; 21]);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct StorageKey(pub [u8; 53]);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct CodeKey(pub [u8; 33]);

pub const ACCOUNT_KEY_PREFIX: u8 = 0x01;
pub const STORAGE_KEY_PREFIX: u8 = 0x02;
pub const CODE_KEY_PREFIX: u8 = 0x03;
pub const ACCOUNT_KEY_LEN: usize = 21;
pub const STORAGE_KEY_LEN: usize = 53;
pub const ACCOUNT_KEY_LEN_U32: u32 = 21;
pub const STORAGE_KEY_LEN_U32: u32 = 53;

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned,
)]
#[repr(C)]
struct AccountKeyWire {
    prefix: u8,
    addr20: [u8; 20],
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned,
)]
#[repr(C)]
struct StorageKeyWire {
    prefix: u8,
    addr20: [u8; 20],
    slot32: [u8; 32],
}

const _: [(); ACCOUNT_KEY_LEN] = [(); size_of::<AccountKeyWire>()];
const _: [(); STORAGE_KEY_LEN] = [(); size_of::<StorageKeyWire>()];
const _: [(); ACCOUNT_KEY_LEN] = [(); size_of::<AccountKey>()];
const _: [(); STORAGE_KEY_LEN] = [(); size_of::<StorageKey>()];

pub fn parse_account_key_bytes(raw: &[u8]) -> Option<[u8; 20]> {
    let wire = AccountKeyWire::read_from_bytes(raw).ok()?;
    if wire.prefix != ACCOUNT_KEY_PREFIX {
        return None;
    }
    Some(wire.addr20)
}

pub fn parse_storage_key_bytes(raw: &[u8]) -> Option<([u8; 20], [u8; 32])> {
    let wire = StorageKeyWire::read_from_bytes(raw).ok()?;
    if wire.prefix != STORAGE_KEY_PREFIX {
        return None;
    }
    Some((wire.addr20, wire.slot32))
}

pub fn make_account_key(addr20: [u8; 20]) -> AccountKey {
    let wire = AccountKeyWire {
        prefix: ACCOUNT_KEY_PREFIX,
        addr20,
    };
    let mut out = [0u8; ACCOUNT_KEY_LEN];
    out.copy_from_slice(wire.as_bytes());
    AccountKey(out)
}

pub fn make_storage_key(addr20: [u8; 20], slot32: [u8; 32]) -> StorageKey {
    let wire = StorageKeyWire {
        prefix: STORAGE_KEY_PREFIX,
        addr20,
        slot32,
    };
    let mut out = [0u8; STORAGE_KEY_LEN];
    out.copy_from_slice(wire.as_bytes());
    StorageKey(out)
}

pub fn make_code_key(code_hash32: [u8; 32]) -> CodeKey {
    let mut buf = [0u8; 33];
    buf[0] = CODE_KEY_PREFIX;
    buf[1..33].copy_from_slice(&code_hash32);
    CodeKey(buf)
}
