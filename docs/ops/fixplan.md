Phase 0: 緊急止血（当日〜即時パッチ）

目的：第三者に止められない状態にする（Critical潰し）

0.1 管理系 update の認可を完全に統一

対象（例）

set_auto_mine

set_mining_interval_ms

set_prune_policy

set_pruning_enabled

（追加で）set_ops_config 等、運用設定を変えるもの全部

実装

ic-evm-wrapper/src/lib.rs に fn ensure_controller() を追加し、管理系update全て冒頭で呼ぶ

可能なら #[update(guard = "ensure_controller_guard")] 型に統一（漏れ防止）

受け入れ条件（AC）

controller以外が呼ぶと必ず unauthorized でtrap

監査観点で「管理系API一覧」が列挙できる状態（README or doc）

Phase 1: 永続肥大化の根絶（High）

目的：“ブロックに乗らない/失敗するTxが永遠に残る” を止める

1.1 Dropped Tx の “墓標設計” か “即時削除” を選ぶ

即削除で

実装方針（即時削除）

evm-core/src/chain.rs（produce_block / select_ready_candidates 等、drop判定が起きる箇所）で drop判定した瞬間に 次を行う：

本文削除（重いデータを消す）

state.tx_store.remove(txid)

Tx本文（RawTx / Envelope / Blob等、サイズが大きいもの）を削除

もし本文が BlobStore 側にもあるなら、対応するポインタもここで削除対象に含める（実装の実態に合わせる）

Dropped状態を記録（軽量メタだけ）

state.tx_locs.insert(txid, TxLoc::Dropped { reason, ts })

reason: drop理由コード（decode error / insufficient gas / fee too low / nonce window 逸脱 等）

ts: ic0::time() で良い（観測用途。不要なら省略してもいいが、あるとデバッグが楽）

関連インデックスを必ず削除（最重要）
Tx本文を消しても、インデックスが残ると「幽霊Tx」が発生して整合性が崩れるので、drop時に必ず掃除する。

txid -> key/meta（例：seen_tx, tx_index, txid_to_queue_key など）

pending_by_sender_nonce から該当エントリ（drop判定の段階が pending/ready どこかにより分岐）

ready_queue から該当エントリ

sender/nonce の補助インデックスがあるならそれも

要するに「このtxidを辿る全ルートを断つ」。削除対象を1箇所に集約するのが事故を減らす。

1.2 Pending/Queue の “無限滞留” をTTLか容量制限で止める

最初は実装が軽い 容量制限（cap） を先に入れて止血、そのあと TTL。

容量制限（先に入れる）

Global cap（全体の pending/ready 合計）

Per-sender cap（送信者ごとの pending数）

cap超過時は Rejected（または evictionポリシーで古い/低feeを落とす）

AC

任意のユーザーが無限に tx を溜められない

Phase 2: IC特有のDoS耐性（High〜Medium）

目的：“無料で殴られて運営がサイクル/命令数を払う” 非対称を縮める

2.1 inspect_message による軽量フィルタ（最優先の防波堤）

やりすぎ注意。重い署名検証はここに入れないのが基本。

canister_inspect_message でやること（軽量のみ）

メソッド名チェック（管理系はここで即rejectでも良い）

payload size / tx size（上限）

形式的に壊れている入力（デコード前段で弾ける範囲）

署名検証・残高検証は原則 submit / produce_block 側へ（段階的に前倒し）

2.2 Nonce window（未来Nonceの無限投入を抑止）

submit_tx 時点で

current_nonce から current_nonce + WINDOW 以内のみ受理（例：+64）

AC

未来Nonceを何万個も積まれて永続化されない

2.3 TTL eviction（Gethライク）

evm-db に (timestamp, txid) 索引（BTree）を追加

定期タスク（timer/heartbeat）で cutoff より古い pending を順に削除

AC

“時間が経てば勝手に掃除される” が成立

Phase 3: 状態整合性と障害復旧（Medium）

目的：“索引ズレ/再起動/upgradeで壊れない” を固める

3.1 不変条件（Invariant）をコード化（デバッグ/テスト）

例

pending_by_sender_nonce にあるtxidは必ず tx_locs か tx_store のどちらかに存在

Dropped墓標の txid は pending/ready に存在しない

実装

debug_assert_invariants()（feature gate でもOK）

プロパティテスト（PBT）でランダム操作列から検証

3.2 upgrade時の挙動を明文化

PendingをHeapに寄せるなら「upgradeで消える」前提をAPIで明示、または pre_upgrade退避（ただし巨大だと危険）

AC

“upgradeでTx消えた” が仕様として説明されている、または消えないよう実装されている

Phase 4: 最適化・簡素化（Low〜Medium）

目的：コスト削減、複雑性削減、運用事故耐性アップ

4.1 get_queue_snapshot(limit) にサーバー側ハードキャップ

limit = min(limit, MAX_SNAPSHOT)

AC：極端なlimitで壊れない

4.2 PruneJournal の扱いを整理

pruneが単発updateで完結するなら削除して簡素化

分割prune（再開が必要）なら “再開点” として残す価値あり

AC：設計方針がドキュメントに落ちている

実装順（現実の手戻りが少ない順）

管理API認可統一（Phase0）

Dropped本文削除（墓標/即時削除）（Phase1.1）

Global/Per-sender cap + nonce window（Phase1.2 + Phase2.2）

inspect_message（軽量フィルタ）（Phase2.1）

TTL eviction（索引 + 定期掃除）（Phase2.3）

不変条件テスト・PBT（Phase3）

query cap / prune整理（Phase4）

仕上げの観点（運用上の最低ライン）

メトリクス：pending数、sender別上位、dropped理由別カウント、stable使用量、cycles残量

緊急停止：controller限定＋できればタイムロック

エラーモデル：Reject理由を体系化（DoS対策は“拒否できること”が武器）
