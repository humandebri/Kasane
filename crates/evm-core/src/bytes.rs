//! どこで: evm-core共通ユーティリティ / 何を: 代表的なbytes変換を集約 / なぜ: 重複実装を減らすため

use alloy_primitives::{B256, U256};

pub fn try_address_to_bytes(address: impl AsRef<[u8]>) -> Result<[u8; 20], &'static str> {
    let src = address.as_ref();
    if src.len() != 20 {
        return Err("address must be 20 bytes");
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(src);
    Ok(out)
}

pub fn b256_to_bytes(value: B256) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(value.as_ref());
    out
}

pub fn u256_to_bytes(value: U256) -> [u8; 32] {
    value.to_be_bytes()
}
