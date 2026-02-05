
---

# 改訂 修正計画（監査全項目網羅・実装形まで）

## ゴール（変えない）

* **最終：フルトラストレス**
* **今：チェーンが止まらず、壊れず、進む**
* **state_root は永続MPTへ**（ただし“順番”が重要）
* **fail-closed は本番前提**（ただし導入ステップは分ける）

---

## フェーズ0：Poison Pill / 永久停止系を先に潰す（最優先）

ここを先にやらないと、MPT入れてもチェーンが死ぬ。

### 0-1. Receipt保存で trap しない（Poison Pill 根絶）

**問題**: `ReceiptLike::to_bytes` がログ数/return_dataで trap → 永久停止
**実装方針（確定）**

* 保存層（evm-db）の Storable 経路で **trap禁止**
* **BlobStore 退避**に統一：`receipt` には `return_data_ptr` / `logs_ptr` を保存
* RPCは巨大データをデフォルトで返さない（必要なら別取得/上限）
  **AC**
* どんな tx でも「保存が原因で」trapしない
* 大ログ tx でもブロック生成が継続する

### 0-2. Pruning crash-consistency の自己矛盾解消

**問題**: blocks.remove後にtrap → journal復旧がload_block依存で詰む
**実装方針（確定）**

* journal に **削除対象TxId一覧（復元可能な最小情報）** を保存
* 順序：`journal write -> 付随データ削除 -> 最後に blocks削除`
  **AC**
* prune途中で落ちても次回起動で必ず回復して前進

### 0-3. mempool jamming（残高不足 + 高fee占有）対策

**実装方針（確定）**

* submit時：`max_cost = gas_limit*max_fee + value` 上界チェックで即reject
* produce時：base_fee/残高で再判定
* candidate選定：sender単位の占有制限（同一senderで枠埋め禁止）
  **AC**
* 残高不足 tx で ready枠が詰まらない

---

## フェーズ1：入口DoS・運用死を止める（P1の“前提”）

### 1-1. `produce_block` 呼び出し制御（allowlist）

**実装方針（確定）**

* `MinerAllowlistV1` を stable に
* `produce_block` 冒頭 `require_miner()?`
* `inspect_message` でも弾く（ただし最終防波堤は `require_miner`）
  **関連バグ**
* `set_miner_allowlist` inspect許可漏れ → **ここで一緒に直す**
  **AC**
* allowlist API が実際に呼べる
* 無権限の produce_block は入口で落ちる（cycle燃やさない）

### 1-2. RPC重クエリの爆発を潰す

* `eth_getBlockByNumber(full_tx=true)` に上限/ページング/禁止のいずれか
* 外部エラーは固定コード、詳細はログのみ（`format!("{:?}", err)`排除）
  **AC**
* 低コストで耐える（攻撃でcycleが溶けない）

### 1-3. RNG経路の封鎖（CI機械検査＋feature固定）

* workspace 全体で禁止経路検査（あなたが既にやってる）
* wasm側 getrandom は custom 以外禁止
  **AC**
* 本番で trap 経路にならない

---

## フェーズ2：state_root を “根本修正” する（永続MPT＋slot差分）

ここがあなたの当初のP1。**ただしフェーズ0/1を先に入れる**のが違い。

### 2-1. 永続MPT（node DB）導入

**実装形（ほぼあなた案でOK）**

* `node_db: StableBTreeMap<B256, NodeVal>`
  ※`NodeVal = { rlp: Bytes, refcnt/epoch: u32 }` を推奨（膨張地獄回避）
* `state_root_meta: { current_root, initialized, schema_version }`
* 「source of truth」を account leaf に寄せる（`storage_root_by_address` はキャッシュ扱い）
  **移行**
* `initialized=false` は **1回だけ再構築**（ただし冪等・進捗保存必須）
  **AC**
* 全件走査経路（旧 `compute_state_root_*`）が本番コードから消える

### 2-2. touched storage を slot 単位で増分反映（TrieDelta）

**実装形**

* `TrieDelta { account_deltas, storage_slot_deltas }`
* コミットパスは1箇所（`trie_commit`）に統一
* `selfdestruct/empty account` は leaf delete + 関連 storage trie 到達不能化
  **AC**
* 大量slotのあるアドレスでも touched slot 数に比例

### 2-3. fail-closed（ただし“検出”を強くする）

**挙動（本番）**

* mismatch → `produce_block` は必ず失敗、ブロック確定しない
* `is_producing`解除、`state_root_mismatch_count++`
* 再現情報（block、2root、delta要約）を stable に保存
  **検証**
* “限定ケース”ではなく **サンプリング検証**（例：1/1024）を推奨
  ＝壊れ始めを早期に捕まえる

---

## フェーズ3：外部互換方針（必要なら）／system tx の扱いを確定

ここは「今すぐ必須」ではないが、監査項目として出てるなら整理が必要。

### 3-1. system tx をブロックに含めるか

* **外部互換を重視するなら**：block body / receipts / index を一貫させて含める
* **重視しないなら**：内部txとして扱い、外部仕様を明確化する
  **AC**
* 仕様が一貫し、インデクサ/RPCが矛盾しない

---

# PR構成（巨大PR地獄を避ける）

あなたの「3点同時」は思想として正しいが、レビュー不能になりがち。現実解：

* **PR-A（フェーズ0）**：Poison Pill + pruning journal + mempool jamming
* **PR-B（フェーズ1）**：allowlist + inspect許可漏れ + RPC制限 + error固定
* **PR-C（フェーズ2）**：永続MPT + TrieDelta + fail-closed（＋サンプリング検証）
* **PR-D（任意）**：system tx / 外部互換整理

こう分けると、**どの時点でもチェーンが死なない**状態を維持しながら進められる。

---

# 受け入れ基準（更新版）

* 保存層で trap が起きない（Poison Pill根絶）
* prune がクラッシュしても復旧する
* mempool jamming が成立しない
* produce_block は許可制・APIがinspectで呼べる
* state_root は全件走査/全storage走査が残っていない
* node_db の増加が制御されている（GC/保持ポリシー）
* mismatch は fail-closed + 再現情報が残る
* 既存テスト全通過＋性能退行が “差分量に比例” を満たす

---

## 要するに

「網羅版」を **優先順位とPR境界で再構成**したのが上の計画。
この形に変えると、あなたが欲しい

* まず動く（死なない）
* でも将来フルトラストレスに接続できる
* 監査指摘を全部潰してる

が全部同時に取れる。

必要なら、あなたのリポ構造（`evm-core/evm-db/wrapper` のどこに何を置くか）に合わせて、各PRの **ファイル単位タスクリスト（差分粒度）**まで落とす。
