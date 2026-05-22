# verus review

重大な指摘なし。

Verus観点:
- 乗算・加算がなく、overflow経路はない。
- postconditionは実装式と同型で、`result == (...)` として直接証明できる。
- `u64` 入力の全域でtotalな関数である。
