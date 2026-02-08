#![no_main]

use evm_db::chain_data::TxLoc;
use ic_stable_structures::Storable;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = TxLoc::from_bytes(std::borrow::Cow::Borrowed(data));
});
