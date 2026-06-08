# implementation review

**Findings**

1. **`did_work == 1` が `previous_present == 0` でも成立する**
   
   条件上、次は通る。

   ```rust
   previous_present = 0
   next_present = 1
   did_work = 1
   ```

   この場合、`previous_boundary < next_boundary` は要求されない。  
   「既存の部分進捗を前進させた」意味なら不正。初回作成も work 扱いなら妥当だが、仕様名 `prune_partial_progress_safe_raw` からは曖昧。

   修正候補:

   ```rust
   && (did_work == 0 || previous_present == 1)
   ```

   ただし「初回進捗作成」を `did_work` に含める設計なら不要。

2. **完了時に progress を削除するケースを拒否する**

   ```rust
   && (previous_present == 0 || next_present == 1)
   ```

   により、次は常に `false`。

   ```rust
   previous_present = 1
   next_present = 0
   ```

   pruning 完了時に progress record を消す設計なら、この述語は「部分進捗継続」専用で、完了ケース用の別述語が必要。  
   完了もこの関数で扱う想定ならバグ。

3. **`stopped_for_budget` は片方向検証**

   現状は「budget stop と主張するなら予算不足」を検証しているだけ。

   ```rust
   stopped_for_budget == 0
   ```

   の場合、実際に予算不足でも許可される。  
   返却値の整合性検証なら十分。停止理由の完全性まで要求するなら不足。

**Edge Cases**

- `next_ops_needed > max_ops` は常に budget stop として許可される。  
  単発操作が予算上限を超えるため安全性としては正しいが、同じ `max_ops` で再実行すると永久停止し得る。liveness 側で別途処理が必要。

- `next_present == 0` の場合、`next_boundary` / `next_cursor` は完全に無視される。  
  absent state の付随値を未定義扱いするなら問題なし。正規化済み値を期待するなら条件不足。

**Verus 観点**

- `max_ops - next_ops_needed` は

  ```rust
  next_ops_needed <= max_ops
  ```

  の右側にだけ出るため、Rust 実行時・Verus とも underflow は避けられる。

- budget 条件は実質これと等価。

  ```rust
  ops_used + next_ops_needed > max_ops
  ```

  ただし加算 overflow を避けるため、現行形は Verus 向き。

- 証明補助には次の形のほうが読みやすい可能性がある。

  ```rust
  max_ops < next_ops_needed || max_ops - next_ops_needed < ops_used
  ```

  `next_ops_needed <= max_ops` 分岐は冗長。ただし現行形は明示性が高く、証明器が扱いやすい場合もある。

**結論**

安全性述語としては概ね堅い。主な確認点は2つ。  
`previous_present == 0 && did_work == 1` を許す仕様か。  
`previous_present == 1 && next_present == 0`、つまり完了削除をこの関数で扱う仕様か。

