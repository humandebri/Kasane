use std::sync::{Mutex, OnceLock};

use evm_db::stable_state::{init_stable_state, with_state_mut, with_state};
use evm_db::chain_data::Head;
use evm_core::chain;

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn need_prune_ignores_enabled_flag() {
    let _guard = test_lock().lock().unwrap();
    init_stable_state();
    with_state_mut(|state| {
        state.head.set(Head { number: 10, block_hash: [0u8; 32], timestamp: 1_000 });
        let mut config = *state.prune_config.get();
        config.pruning_enabled = false;
        config.retain_days = 1;
        config.set_oldest(0, 0);
        state.prune_config.set(config);
    });
    let status = chain::get_prune_status();
    assert!(status.need_prune, "need_prune should be true even when disabled");
}

#[test]
fn retain_blocks_keeps_exact_count() {
    let _guard = test_lock().lock().unwrap();
    init_stable_state();
    with_state_mut(|state| {
        state.head.set(Head { number: 10, block_hash: [0u8; 32], timestamp: 1_000 });
        let mut config = *state.prune_config.get();
        config.pruning_enabled = true;
        config.retain_blocks = 3;
        config.retain_days = 0;
        config.set_oldest(0, 0);
        state.prune_config.set(config);
    });
    let result = chain::prune_tick().expect("prune_tick should succeed");
    assert!(result.pruned_before_block.is_some());
    assert_eq!(result.pruned_before_block.unwrap(), 7);
}
