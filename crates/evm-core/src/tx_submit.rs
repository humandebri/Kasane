//! どこで: submit時の共通ルール / 何を: nonce運用と置換判定 / なぜ: 二重実装の漏れを防ぐため

use crate::revm_exec::compute_effective_gas_price;
use evm_db::chain_data::{SenderKey, StoredTx, TxId};
use evm_db::stable_state::StableState;
use evm_db::types::keys::make_account_key;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NonceRuleError {
    TooLow,
    Gap,
    Conflict,
}

pub fn expected_nonce_for_sender(state: &mut StableState, sender: SenderKey) -> u64 {
    // gap不許可のため、submit時に期待nonceを固定し、実行/ドロップ時にのみ進める。
    if let Some(value) = state.sender_expected_nonce.get(&sender) {
        return value;
    }
    let nonce = account_nonce_from_state(state, sender);
    state.sender_expected_nonce.insert(sender, nonce);
    nonce
}

pub fn finalize_pending_for_sender(state: &mut StableState, sender: SenderKey, tx_id: TxId) {
    if let Some(current) = state.pending_current_by_sender.get(&sender) {
        if current == tx_id {
            state.pending_current_by_sender.remove(&sender);
            bump_expected_nonce(state, sender);
        }
    }
}

pub fn apply_nonce_and_replacement(
    state: &mut StableState,
    sender: SenderKey,
    nonce: u64,
    effective_gas_price: u64,
    base_fee: u64,
) -> Result<Option<TxId>, NonceRuleError> {
    let expected_nonce = expected_nonce_for_sender(state, sender);
    if nonce < expected_nonce {
        return Err(NonceRuleError::TooLow);
    }
    if nonce > expected_nonce {
        return Err(NonceRuleError::Gap);
    }
    if let Some(old_tx_id) = state.pending_current_by_sender.get(&sender) {
        let old_effective = effective_gas_price_for_tx(state, old_tx_id, base_fee)?;
        if effective_gas_price <= old_effective {
            return Err(NonceRuleError::Conflict);
        }
        return Ok(Some(old_tx_id));
    }
    Ok(None)
}

fn account_nonce_from_state(state: &StableState, sender: SenderKey) -> u64 {
    // IcSynthetic/Eth共通のexpected_nonce初期化に使う（EVM stateのnonce）
    let key = make_account_key(sender.0);
    state
        .accounts
        .get(&key)
        .map(|value| value.nonce())
        .unwrap_or(0)
}

fn bump_expected_nonce(state: &mut StableState, sender: SenderKey) {
    let current = state
        .sender_expected_nonce
        .get(&sender)
        .unwrap_or_else(|| account_nonce_from_state(state, sender));
    state
        .sender_expected_nonce
        .insert(sender, current.saturating_add(1));
}

fn effective_gas_price_for_tx(
    state: &StableState,
    tx_id: TxId,
    base_fee: u64,
) -> Result<u64, NonceRuleError> {
    let envelope = state.tx_store.get(&tx_id).ok_or(NonceRuleError::Conflict)?;
    let stored = StoredTx::try_from(envelope).map_err(|_| NonceRuleError::Conflict)?;
    let max_fee_per_gas = stored.max_fee_per_gas;
    let max_priority_fee_per_gas = stored.max_priority_fee_per_gas;
    let is_dynamic_fee = stored.is_dynamic_fee;
    compute_effective_gas_price(
        max_fee_per_gas,
        if is_dynamic_fee {
            max_priority_fee_per_gas
        } else {
            0
        },
        base_fee,
    )
    .ok_or(NonceRuleError::Conflict)
}
