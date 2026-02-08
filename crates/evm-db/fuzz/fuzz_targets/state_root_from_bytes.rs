#![no_main]

use evm_db::chain_data::{GcStateV1, HashKey, MigrationStateV1, NodeRecord, StateRootMetaV1};
use ic_stable_structures::Storable;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = HashKey::from_bytes(std::borrow::Cow::Borrowed(data));
    let _ = NodeRecord::from_bytes(std::borrow::Cow::Borrowed(data));
    let _ = GcStateV1::from_bytes(std::borrow::Cow::Borrowed(data));
    let _ = MigrationStateV1::from_bytes(std::borrow::Cow::Borrowed(data));
    let _ = StateRootMetaV1::from_bytes(std::borrow::Cow::Borrowed(data));
});
