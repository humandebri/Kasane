//! どこで: Phase1のCommitter / 何を: Overlayの書き戻し / なぜ: 決定的な永続化のため

use evm_db::overlay::OverlayMap;
use evm_db::stable_state::with_state_mut;
use evm_db::types::keys::{AccountKey, CodeKey, StorageKey};
use evm_db::types::values::{AccountVal, CodeVal, U256Val};

pub fn commit_accounts(overlay: &mut OverlayMap<AccountKey, AccountVal>) {
    with_state_mut(|state| {
        overlay.drain_to(|key, value| match value {
            Some(v) => {
                state.accounts.insert(key, v);
            }
            None => {
                state.accounts.remove(&key);
            }
        });
    });
}

pub fn commit_storage(overlay: &mut OverlayMap<StorageKey, U256Val>) {
    with_state_mut(|state| {
        overlay.drain_to(|key, value| match value {
            Some(v) => {
                state.storage.insert(key, v);
            }
            None => {
                state.storage.remove(&key);
            }
        });
    });
}

pub fn commit_codes(overlay: &mut OverlayMap<CodeKey, CodeVal>) {
    with_state_mut(|state| {
        overlay.drain_to(|key, value| match value {
            Some(v) => {
                state.codes.insert(key, v);
            }
            None => {
                state.codes.remove(&key);
            }
        });
    });
}
