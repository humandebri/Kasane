use std::sync::{Mutex, OnceLock};

use evm_core::chain;
use evm_db::chain_data::{BlockData, Head, PrunePolicy};
use evm_db::stable_state::{init_stable_state, with_state_mut};
use evm_db::Storable;

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn need_prune_ignores_enabled_flag() {
    let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
    init_stable_state();
    with_state_mut(|state| {
        state.head.set(Head {
            number: 10,
            block_hash: [0u8; 32],
            timestamp: 86_401,
        });
        let mut config = *state.prune_config.get();
        config.pruning_enabled = false;
        config.retain_days = 1;
        config.set_oldest(0, 0);
        state.prune_config.set(config);
    });
    let status = chain::get_prune_status();
    assert!(
        status.need_prune,
        "need_prune should be true even when disabled"
    );
}

#[test]
fn retain_blocks_keeps_exact_count() {
    let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
    init_stable_state();
    with_state_mut(|state| {
        for number in 0..=10 {
            let block = make_block(number);
            insert_block(state, number, &block);
        }
        state.head.set(Head {
            number: 10,
            block_hash: [0u8; 32],
            timestamp: 86_401,
        });
        let mut config = *state.prune_config.get();
        config.pruning_enabled = true;
        config.set_policy(PrunePolicy {
            target_bytes: 0,
            retain_days: 1,
            retain_blocks: 3,
            headroom_ratio_bps: 2000,
            hard_emergency_ratio_bps: 9500,
            max_ops_per_tick: 5_000,
        });
        config.set_oldest(0, 0);
        state.prune_config.set(config);
    });
    let result = chain::prune_tick().expect("prune_tick should succeed");
    assert!(result.pruned_before_block.is_some());
    assert_eq!(result.pruned_before_block.unwrap(), 7);
}

fn make_block(number: u64) -> BlockData {
    let parent_hash = [0u8; 32];
    let number_u8 = u8::try_from(number).unwrap_or(0);
    let block_hash = [number_u8; 32];
    let tx_list_hash = [number_u8; 32];
    let state_root = [0u8; 32];
    BlockData::new(
        number,
        parent_hash,
        block_hash,
        number,
        1_000_000_000,
        3_000_000,
        0,
        Vec::new(),
        tx_list_hash,
        state_root,
    )
}

fn insert_block(state: &mut evm_db::stable_state::StableState, number: u64, block: &BlockData) {
    let bytes = block.to_bytes().into_owned();
    let ptr = state.blob_store.store_bytes(&bytes).expect("store block");
    state.blocks.insert(number, ptr);
}
