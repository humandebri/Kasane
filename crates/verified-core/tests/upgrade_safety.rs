//! どこで: verified-core upgrade safety / 何を: 観測保持の純粋条件 / なぜ: adapter evidence の比較結果を固定するため

use verified_core::upgrade_safety::upgrade_core_observation_preserved_raw;

#[test]
fn upgrade_observation_requires_all_core_fields_preserved() {
    assert!(upgrade_core_observation_preserved_raw(1, 1, 1, 1, 1, 1));
    assert!(!upgrade_core_observation_preserved_raw(0, 1, 1, 1, 1, 1));
    assert!(!upgrade_core_observation_preserved_raw(1, 0, 1, 1, 1, 1));
    assert!(!upgrade_core_observation_preserved_raw(1, 1, 0, 1, 1, 1));
    assert!(!upgrade_core_observation_preserved_raw(1, 1, 1, 0, 1, 1));
    assert!(!upgrade_core_observation_preserved_raw(1, 1, 1, 1, 0, 1));
    assert!(!upgrade_core_observation_preserved_raw(1, 1, 1, 1, 1, 0));
    assert!(!upgrade_core_observation_preserved_raw(2, 1, 1, 1, 1, 1));
    assert!(!upgrade_core_observation_preserved_raw(
        1,
        1,
        1,
        1,
        1,
        u64::MAX
    ));
}
