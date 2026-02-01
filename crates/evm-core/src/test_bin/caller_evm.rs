//! どこで: 開発用CLI / 何を: principal文字列 -> caller_evm / なぜ: canister外で導出するため

use tiny_keccak::{Hasher, Keccak};

const DOMAIN_SEP: &[u8] = b"ic-evm:caller_evm:v1";

fn main() {
    let principal = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: caller_evm <principal_text>");
        std::process::exit(1);
    });
    let bytes = match decode_principal_text(&principal) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("invalid principal: {err}");
            std::process::exit(1);
        }
    };
    let mut out = [0u8; 32];
    let mut hasher = Keccak::v256();
    hasher.update(DOMAIN_SEP);
    hasher.update(&bytes);
    hasher.finalize(&mut out);
    let addr = &out[12..32];
    println!("{}", hex_encode(addr));
}

fn decode_principal_text(text: &str) -> Result<Vec<u8>, &'static str> {
    let mut cleaned = String::with_capacity(text.len());
    for c in text.chars() {
        if c != '-' {
            cleaned.push(c.to_ascii_uppercase());
        }
    }
    let mut bits: u32 = 0;
    let mut bits_left: u8 = 0;
    let mut out = Vec::new();
    for ch in cleaned.chars() {
        let val = base32_value(ch).ok_or("invalid base32")?;
        bits = (bits << 5) | (val as u32);
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            let byte = (bits >> bits_left) as u8;
            out.push(byte);
            bits &= (1u32 << bits_left) - 1;
        }
    }
    if out.len() < 4 {
        return Err("too short");
    }
    // 先頭4byteはCRCなので捨てる（検証は省略）
    Ok(out[4..].to_vec())
}

fn base32_value(ch: char) -> Option<u8> {
    match ch {
        'A'..='Z' => Some((ch as u8) - b'A'),
        '2'..='7' => Some((ch as u8) - b'2' + 26),
        _ => None,
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}
