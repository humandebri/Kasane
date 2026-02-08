#![no_main]

use evm_db::types::keys::{parse_account_key_bytes, parse_storage_key_bytes};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = parse_account_key_bytes(data);
    let _ = parse_storage_key_bytes(data);
});
