# verus review

指摘: `pub fn` として公開するなら、検証対象が実データではなく「呼出元が渡したメタ値」だけになる。攻撃的呼出元は `target_non_anonymous = 1` や `method_ascii = 1` を任意に偽装できるため、この関数単体は principal/method の安全性を保証しない。実データから `len/non_anonymous/ascii` を同一スコープで導出する wrapper を公開し、raw 版は `pub(crate)` 以下にするのが安全。

Verus 仕様と実装の式は一致している。境界値も自然: `target_len == 1/MAX_PRINCIPAL_LEN` は許可、`0/MAX+1` は拒否。`method_len` も同様。`u64` 比較のみなのでオーバーフロー余地はない。

確認点:
- `MAX_PRINCIPAL_LEN` と `MAX_QUERY_METHOD_LEN` が `u64` 互換であること。
- `verus_spec(valid => ensures ...)` がこのリポの属性マクロで戻り値名 `valid` を正しく束縛すること。specgen 標準の固定戻り値名は `result` なので、通常の specgen 注入と混在するなら要確認。
- テストは境界値と偽装フラグを明示: flag が `0/2/u64::MAX` の場合は拒否、len 境界は inclusive。
