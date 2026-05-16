# verus review

**所見**

- Medium: [upgrade_safety.rs](/Users/0xhude/Desktop/ICP/Kasane/crates/verified-core/src/upgrade_safety.rs:6) に Verus 契約が未接続。`vstd::prelude::*` はあるが、`#[cfg_attr(verus_keep_ghost, verus_spec(...))]` と `specgen:contract` がないため、意図する `result <==> 全入力 == 1` を Verus で証明対象にできない。既存の `stable_namespace.rs` などと同じ形で contract attr を追加する必要がある。

- Low: [upgrade_safety.rs test](/Users/0xhude/Desktop/ICP/Kasane/crates/verified-core/tests/upgrade_safety.rs:8) は `0` の拒否だけを検査している。敵対入力では `2` や `u64::MAX` が重要。実装が誤って `!= 0` に変わっても現テストでは検出できない。`(2,1,1,1,1,1)` と `(1,1,1,1,1,u64::MAX)` を追加すべき。

実装本体の述語は意図どおり。副作用、panic、overflow はない。`evm-db` 側の呼び出しも `u64::from(bool)` なので通常経路では `0/1` に限定される。検証コマンドは実行していない。

