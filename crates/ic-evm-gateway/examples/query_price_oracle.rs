//! Price fixture distinguishes consensus execution from nonreplicated query evaluation.
#[ic_cdk::query]
fn price() -> u64 {
    if ic_cdk::api::in_replicated_execution() {
        42
    } else {
        41
    }
}
