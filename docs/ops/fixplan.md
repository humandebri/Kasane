# fixplan（運用・防御補助計画）

この文書は `docs/ops/fixplan2.md` を最優先にした補助計画です。  
**実装順・仕様凍結の主導は fixplan2（PR0〜PR9）** とし、本書は運用防御と事故防止の観点を補います。

---

## 0. 優先順位ルール（更新）

1. **fixplan2.md が正（Source of Truth）**
2. 本書は矛盾しない範囲でのみ有効
3. 矛盾した場合は本書側を修正する

---

## 1. 認可・API方針（fixplan2に合わせて改訂）

### 1.1 管理系 update の認可統一

対象例:
- `set_auto_mine`
- `set_mining_interval_ms`
- `set_prune_policy`
- `set_pruning_enabled`
- `set_ops_config` など運用設定変更API

実装方針:
- `ic-evm-wrapper/src/lib.rs` で管理系APIの認可を統一
- guard関数で漏れを防ぐ

### 1.2 エラー返却方針（trap乱用を避ける）

旧方針の「unauthorizedでtrap」は撤回。  
fixplan2（PR7: 安定分類）に合わせ、**外部APIは安定したエラー分類を返す**。

原則:
- 入力不正/権限不足/業務拒否は分類済みエラーで返す
- trapは不変条件違反・破損検知など内部異常に限定

---

## 2. 永続肥大化・メモリ圧迫対策（fixplan2と整合）

### 2.1 Dropped Tx の扱い

方針:
- drop確定時に重い本文を削除（必要なら軽量墓標のみ残す）
- 関連インデックスを同時に掃除し、幽霊参照を禁止

注意:
- `StoredTx`/index設計は PR1 の型整理後に最終反映
- 実装位置は `chain.rs` の drop確定経路に集約

### 2.2 滞留対策（cap優先、TTL後追い）

優先度:
1. Global cap / Per-sender cap
2. nonce window
3. TTL eviction

理由:
- 先に「無限投入できない」状態を作るほうが止血効果が高い

---

## 3. IC特有のDoS防御（fixplan2 PR8と非衝突）

### 3.1 inspect_message は軽量のみ

許可:
- メソッド名
- payload/txサイズ
- 形式の軽量チェック

非推奨:
- 重い署名検証
- 状態依存の高コスト検証

### 3.2 検証責務の境界

fixplan2 PR8に合わせて以下を明確化:
- Ingress入口: 形式/サイズ/基本妥当性
- EVM内部: precompile責務（例: `ecrecover`）
- 二重実装は禁止

---

## 4. 整合性・復旧（PR0/PR5に合わせる）

### 4.1 不変条件のコード化

例:
- pending参照が孤立しない
- dropped tx が pending/ready に残らない

実施:
- debug用 invariant check
- ランダム操作列テスト（PBT）

### 4.2 upgrade方針の固定

原則:
- `MemoryId` は変更しない（追加のみ）
- `Storable` 変更は versioned encoding
- upgrade前後互換テストを更新

---

## 5. 実装順（fixplan2優先で再編）

### 最優先（fixplan2と並走で即対応）

1. 管理API認可統一（本書 1.1）
2. エラー返却方針の統一（本書 1.2、PR7準備）
3. 入力サイズ上限と snapshot hard cap

### fixplan2 PR進行中に差し込む項目

- PR1〜PR3中: dropped掃除・cap/nonce window の反映点を確定
- PR3〜PR5中: invariantチェックと互換テスト更新
- PR7〜PR8中: エラーマッピング表と検証責務境界を確定

### 後段

- TTL eviction
- pruning運用最適化
- 監視メトリクス強化

---

## 6. 受け入れ条件（改訂）

- fixplan2のPR順を崩さずに本書の防御項目が入る
- 管理APIで権限制御漏れがない
- unauthorized/invalid input が分類済みエラーで観測できる
- queue/pendingが無限増加しない
- dropped後に孤立インデックスが残らない
- upgrade前後で互換テストが通る

---

## 7. 運用最低ライン

- メトリクス: pending数、sender上位、dropped理由、stable使用量、cycles残量
- 緊急停止: controller限定、運用手順をRunbook化
- 障害時: 「拒否理由が分類されている」ことを最優先で確認
