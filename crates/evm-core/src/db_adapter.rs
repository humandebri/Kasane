//! どこで: Phase1のDBアダプタ / 何を: StableState読み取りAPI / なぜ: REVM接続の土台にするため

use evm_db::stable_state::with_state;
use evm_db::types::keys::{AccountKey, CodeKey, StorageKey};
use evm_db::types::values::{AccountVal, CodeVal, U256Val};

#[derive(Clone, Copy, Debug)]
pub struct StableDbAdapter;

impl StableDbAdapter {
    pub fn get_account(&self, key: &AccountKey) -> Option<AccountVal> {
        with_state(|state| state.accounts.get(key))
    }

    pub fn get_storage(&self, key: &StorageKey) -> Option<U256Val> {
        with_state(|state| state.storage.get(key))
    }

    pub fn get_code(&self, key: &CodeKey) -> Option<CodeVal> {
        with_state(|state| state.codes.get(key))
    }
}
