//! どこで: Storable::from_bytes の安全デコード / 何を: 境界チェックとフォールバック / なぜ: 破損データでもTrapしないため

use tiny_keccak::{Hasher, Keccak};

pub fn read_exact<'a>(data: &'a [u8], offset: &mut usize, len: usize) -> Option<&'a [u8]> {
    if *offset > data.len() {
        return None;
    }
    let remaining = data.len() - *offset;
    if len > remaining {
        return None;
    }
    let start = *offset;
    let end = start + len;
    *offset = end;
    Some(&data[start..end])
}

pub fn read_u8(data: &[u8], offset: &mut usize) -> Option<u8> {
    let slice = read_exact(data, offset, 1)?;
    Some(slice[0])
}

pub fn read_u32(data: &[u8], offset: &mut usize) -> Option<u32> {
    let slice = read_exact(data, offset, 4)?;
    let mut buf = [0u8; 4];
    buf.copy_from_slice(slice);
    Some(u32::from_be_bytes(buf))
}

pub fn read_u64(data: &[u8], offset: &mut usize) -> Option<u64> {
    let slice = read_exact(data, offset, 8)?;
    let mut buf = [0u8; 8];
    buf.copy_from_slice(slice);
    Some(u64::from_be_bytes(buf))
}

pub fn read_array<const N: usize>(data: &[u8], offset: &mut usize) -> Option<[u8; N]> {
    let slice = read_exact(data, offset, N)?;
    let mut out = [0u8; N];
    out.copy_from_slice(slice);
    Some(out)
}

pub fn read_vec(data: &[u8], offset: &mut usize, len: usize) -> Option<Vec<u8>> {
    let slice = read_exact(data, offset, len)?;
    Some(slice.to_vec())
}

pub fn hash_to_array<const N: usize>(label: &[u8], data: &[u8]) -> [u8; N] {
    let mut hash = [0u8; 32];
    let mut hasher = Keccak::v256();
    hasher.update(label);
    hasher.update(data);
    hasher.finalize(&mut hash);
    let mut out = [0u8; N];
    let copy_len = if N < hash.len() { N } else { hash.len() };
    out[..copy_len].copy_from_slice(&hash[..copy_len]);
    out
}
