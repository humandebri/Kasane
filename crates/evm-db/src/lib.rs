//! Phase0の土台（どこで: canister入口 / 何を: 初期化とupgradeフック / なぜ: Stable Memory凍結を守るため）

pub mod memory;
pub mod meta;
pub mod overlay;
pub mod phase1;
pub mod stable_state;
pub mod types;
pub mod upgrade;

use meta::init_meta_or_trap;
use stable_state::init_stable_state;

#[ic_cdk::init]
fn init() {
    init_meta_or_trap();
    init_stable_state();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    upgrade::post_upgrade();
    init_meta_or_trap();
    init_stable_state();
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    // Phase0では軽量設定のみ退避する前提。詳細は upgrade.rs に集約する。
    upgrade::pre_upgrade();
}
