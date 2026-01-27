//! どこで: Phase1のCommitter / 何を: Overlayの書き戻し / なぜ: 決定的な永続化のため

use evm_backend::overlay::OverlayMap;
use evm_backend::stable_state::with_state_mut;
use evm_backend::types::keys::{AccountKey, CodeKey, StorageKey};
use evm_backend::types::values::{AccountVal, CodeVal, U256Val};

pub fn commit_accounts(overlay: &OverlayMap<AccountKey, AccountVal>) {
    with_state_mut(|state| {
        overlay.commit_to(|key, value| match value {
            Some(v) => {
                state.accounts.insert(*key, *v);
            }
            None => {
                state.accounts.remove(key);
            }
        });
    });
}

pub fn commit_storage(overlay: &OverlayMap<StorageKey, U256Val>) {
    with_state_mut(|state| {
        overlay.commit_to(|key, value| match value {
            Some(v) => {
                state.storage.insert(*key, *v);
            }
            None => {
                state.storage.remove(key);
            }
        });
    });
}

pub fn commit_codes(overlay: &OverlayMap<CodeKey, CodeVal>) {
    with_state_mut(|state| {
        overlay.commit_to(|key, value| match value {
            Some(v) => {
                state.codes.insert(*key, v.clone());
            }
            None => {
                state.codes.remove(key);
            }
        });
    });
}
