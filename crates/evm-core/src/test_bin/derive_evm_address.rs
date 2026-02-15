//! どこで: 開発用CLI / 何を: principal文字列 -> EVMアドレス / なぜ: canister外で導出するため

use candid::Principal;
use ic_evm_address::derive_evm_address_from_principal;

fn main() {
    let principal = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: derive_evm_address <principal_text>");
        std::process::exit(1);
    });
    let bytes = match decode_principal_text(&principal) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("invalid principal: {err}");
            std::process::exit(1);
        }
    };
    let addr = match derive_evm_address_from_principal(bytes.as_slice()) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("failed to derive EVM address: {err:?}");
            std::process::exit(1);
        }
    };
    println!("{}", hex::encode(addr));
}

fn decode_principal_text(text: &str) -> Result<Vec<u8>, String> {
    Principal::from_text(text)
        .map(|principal| principal.as_slice().to_vec())
        .map_err(|err| err.to_string())
}
