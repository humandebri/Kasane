//! どこで: Phase1のSELFDESTRUCT対応 / 何を: storage全削除 + account削除 / なぜ: EVM互換の最低限

use evm_db::stable_state::with_state_mut;
use evm_db::types::keys::{make_account_key, make_code_key, make_storage_key, StorageKey};

pub fn selfdestruct_address(addr20: [u8; 20]) {
    with_state_mut(|state| {
        let account_key = make_account_key(addr20);
        if let Some(account) = state.accounts.get(&account_key) {
            let code_hash = account.code_hash();
            let code_key = make_code_key(code_hash);
            state.codes.remove(&code_key);
        }
        state.accounts.remove(&account_key);

        let start = make_storage_key(addr20, [0x00u8; 32]);
        let end = make_storage_key(addr20, [0xFFu8; 32]);
        let mut keys: Vec<StorageKey> = Vec::new();
        for entry in state.storage.range((
            std::ops::Bound::Included(start),
            std::ops::Bound::Included(end),
        )) {
            keys.push(*entry.key());
        }
        for key in keys.into_iter() {
            state.storage.remove(&key);
        }
    });
}
