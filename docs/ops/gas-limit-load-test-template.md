## BLOCK_GAS_LIMIT 負荷計測テンプレート

目的:
- `DEFAULT_BLOCK_GAS_LIMIT` の適正値を staging 実測で決める。

前提:
- 候補値: `2_000_000 / 2_500_000 / 3_000_000 / 3_500_000 / 4_000_000`
- 同一ワークロードを全候補で実行する。
- 各候補は最低3回測定する。

---

### 1) 事前準備チェック

- [ ] 計測対象コミットSHA:
- [ ] 計測日:
- [ ] 環境: `staging`
- [ ] ワークロード定義（投入tx種別、件数、送信レート）:
- [ ] canister version / wasm hash:

---

### 2) ローカル事前確認（任意）

```bash
cd /Users/0xhude/Desktop/ICP/Kasane
canbench
canbench --persist
```

メモ:
- `produce_block_path` と `submit_ic_tx_path` の instructions/heap/stable を確認する。

ローカル基準値（2026-02-08, `canbench_results.yml`）:

| bench | calls | instructions | heap_increase(pages) | stable_memory_increase(pages) |
|---|---:|---:|---:|---:|
| produce_block_path | 1 | 1,480,196 | 5 | 128 |
| submit_ic_tx_path | 1 | 537,645 | 0 | 0 |

注記:
- これは block gas limit 候補比較ではなく、現行実装のローカル計測スナップショット。
- 採用判定は必ず staging の候補比較表（次セクション）で行う。

---

### 3) staging 計測手順（候補ごと）

1. `crates/evm-db/src/chain_data/constants.rs` の `DEFAULT_BLOCK_GAS_LIMIT` を候補値に変更
2. wasm ビルド・デプロイ
3. 同一ワークロードを3回実行
4. 各回で以下を記録
   - `produce_block` 成否
   - 1ブロック処理時間（ms）
   - cycles 差分
   - drop率（dropped/total）
5. 次の候補値に進む

---

### 4) 記録表（候補×反復）

| candidate_gas_limit | run | produce_block_success | p95_block_time_ms | cycles_delta | total_submitted | total_included | total_dropped | drop_rate |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| 2_000_000 | 1 |  |  |  |  |  |  |  |
| 2_000_000 | 2 |  |  |  |  |  |  |  |
| 2_000_000 | 3 |  |  |  |  |  |  |  |
| 2_500_000 | 1 |  |  |  |  |  |  |  |
| 2_500_000 | 2 |  |  |  |  |  |  |  |
| 2_500_000 | 3 |  |  |  |  |  |  |  |
| 3_000_000 | 1 |  |  |  |  |  |  |  |
| 3_000_000 | 2 |  |  |  |  |  |  |  |
| 3_000_000 | 3 |  |  |  |  |  |  |  |
| 3_500_000 | 1 |  |  |  |  |  |  |  |
| 3_500_000 | 2 |  |  |  |  |  |  |  |
| 3_500_000 | 3 |  |  |  |  |  |  |  |
| 4_000_000 | 1 |  |  |  |  |  |  |  |
| 4_000_000 | 2 |  |  |  |  |  |  |  |
| 4_000_000 | 3 |  |  |  |  |  |  |  |

計算:
- `drop_rate = total_dropped / total_submitted`

---

### 5) 採用判定

1. `produce_block_success=true` が全反復で満たされる候補を抽出
2. その中から最大 `candidate_gas_limit` を基準値に選ぶ
3. 基準値に 20% ヘッドルームを適用して採用値を決定

記録:
- 基準値:
- 20%ヘッドルーム後の採用値:
- 最終採用 `DEFAULT_BLOCK_GAS_LIMIT`:

---

### 6) 反映チェック

- [ ] `crates/evm-db/src/chain_data/constants.rs` の `DEFAULT_BLOCK_GAS_LIMIT` 更新
- [ ] `crates/evm-core/src/revm_exec.rs` で block header 反映を確認
- [ ] `crates/evm-core/src/chain.rs` で base_fee 計算に同値が使われることを確認
- [ ] 回帰テスト実施（`cargo test -p evm-db` / `cargo test -p ic-evm-core`）
