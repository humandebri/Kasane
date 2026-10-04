// Compile the production modules directly; do not copy their implementation.
#[path = "../../../crates/verified-core/src/state_diff.rs"]
pub mod state_diff;

#[path = "../../../crates/verified-core/src/fee.rs"]
pub mod fee;
#[path = "../../../crates/verified-core/src/unwrap_dispatch.rs"]
pub mod unwrap_dispatch;
