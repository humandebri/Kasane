ご提示いただいたソースコード、設定、およびドキュメントに基づき、シニアセキュリティ＆信頼性レビューアとして監査を行います。

本プロジェクトは、Rust実装のEVM（revm）をInternet Computer（IC）のStable Memory上で動作させるL2/サイドチェーンソリューションのPoC（Proof of Concept）フェーズの実装と見受けられます。

以下に監査結果を報告します。

### 1) Executive Summary
本プロジェクトは、ic-stable-structuresを活用し、EVMの状態を永続メモリ（Stable Memory）に直接マッピングする意欲的な設計です。しかし、**ブロック生成およびステートルート計算において、データ量に比例して処理時間が増大するO(N)の操作が含まれており、実運用レベルのデータ量（数千アカウント/数万トランザクション）に達するとInstruction Limitに抵触し、システムが停止（Trap）する「計算量的な時限爆弾」を抱えています。** また、データのデシリアライズ処理（Storable::from_bytes）でtrap（強制終了）を多用しており、万が一データ破損やバグで不正なデータが書き込まれた場合、キャニスターがリカバリ不能になるリスクが高い設計です。これらはPhase 1（実行基盤）としても重大なブロッカーです。

---

### 2) 重大指摘（Critical/High）

#### 1. 全状態走査によるState Root計算（DoS/スケーラビリティ欠陥）
*   **Severity:** **Critical** / **Confidence:** High
*   **File:** /crates/evm-core/src/state_root.rs
*   **Evidence:**
    
rust
    pub fn compute_state_root_with(state: &StableState) -> [u8; 32] {
        let mut acc = Vec::new();
        for entry in state.accounts.iter() {
    // ... (accounts, storage, codes を全てイテレート)

*   **Impact:** produce_blockのたびに、保存されている**全て**のアカウント、ストレージスロット、コードを読み出してハッシュ計算しています。ICの1メッセージあたりの命令数制限（Instruction Limit）により、ステートサイズがある閾値を超えた瞬間、ブロック生成が永久にTrapし、チェーンが停止します。
*   **Fix:**
    *   Merkle Patricia Trie (MPT) または Verkle Tree を実装し、変更があった部分のみハッシュを再計算する構造にする。
    *   PoC段階で厳密さが不要なら、一時的にステート全体のハッシュ計算を無効化し、トランザクションリストのハッシュのみをブロックに含める。

#### 2. Mempool再計算時の全件ロード（DoS/スケーラビリティ欠陥）
*   **Severity:** **Critical** / **Confidence:** High
*   **File:** /crates/evm-core/src/chain.rs
*   **Evidence:**
    
rust
    fn rekey_ready_queue_with_drop(...) {
        let mut keys: Vec<ReadyKey> = Vec::new();
        for entry in state.ready_queue.range(..) { // 全件取得
            keys.push(*entry.key());
        }
    // ... その後、全件に対して個別にgetして処理

*   **Impact:** base_feeが変動するたびに実行されるこの処理は、Mempool（ready_queue）内の全トランザクションをヒープにロードし、再評価しています。Pendingトランザクションが増加すると、produce_blockがタイムアウト（Limit Exceeded）し、新しいブロックが生成できなくなります。
*   **Fix:**
    *   全件走査を避け、先頭からmax_txs分だけ評価する、あるいはインデックス構造を見直して影響を受けるTxのみを再計算するロジックに変更する。

#### 3. Storable::from_bytes 内での Trap 多用（リカバリ不能リスク）
*   **Severity:** **High** / **Confidence:** High
*   **File:** /crates/evm-db/src/chain_data/receipt.rs (他、block.rs, tx.rs等多数)
*   **Evidence:**
    
rust
    if data.len() > RECEIPT_MAX_SIZE_U32 as usize {
        ic_cdk::trap("receipt: invalid length");
    }
    // ...
    ic_cdk::trap("receipt: invalid return_data length");

*   **Impact:** ic-stable-structuresのfrom_bytesはデータ読み出し時に呼ばれます。ここでtrapすると、そのデータ構造へのアクセス手段が完全に失われます（読み出そうとすると必ず落ちる）。バグやアップグレード時の仕様変更で不整合なデータが一つでも混入すると、マイグレーションすらできなくなります。
*   **Fix:**
    *   from_bytes内では絶対にtrapせず、デフォルト値やOption型でのラップ、あるいは「壊れたデータ」を示す特殊なEnumバリアントを返すように設計を変更する。データの整合性チェックは書き込み時（to_bytes前）に厳格に行う。

---

### 3) 中〜軽微指摘（Medium/Low）

#### 4. Wasmターゲットでの乱数生成失敗（機能不全）
*   **Severity:** Medium
*   **File:** /crates/ic-evm-wrapper/src/lib.rs
*   **Evidence:**
    
rust
    #[cfg(target_arch = "wasm32")]
    fn always_fail_getrandom(_buf: &mut [u8]) -> Result<(), getrandom::Error> {
        Err(getrandom::Error::UNSUPPORTED)
    }

*   **Impact:** 依存クレート（alloy-signer, k256 等）が内部で乱数を必要とするコードパスを通った場合、実行時エラーとなります。署名の検証（recover）だけであれば乱数は不要な場合が多いですが、鍵生成や一部の署名処理が含まれるとパニックします。
*   **Fix:** ICのManagement Canisterのraw_randをシードとするPRNGを実装するか、乱数を必要とする依存機能をfeature flag等で無効化する。

#### 5. Principal長のハードコード制限（将来的な互換性）
*   **Severity:** Low
*   **File:** /crates/evm-db/src/chain_data/caller.rs
*   **Evidence:** if bytes.len() > MAX_PRINCIPAL_LEN { ic_cdk::trap(...) } (29 bytes)
*   **Impact:** 現在のIC仕様ではPrincipalは最大29バイトですが、内部仕様では可変長であり、将来的に拡張された場合にこのキャニスターはそれらのPrincipalを受け付けられなくなります。
*   **Fix:** 固定長配列ではなく、可変長（Blob等）として扱うか、ハッシュ化して固定長IDとして管理する（EVMアドレス生成時と同様のアプローチ）。

#### 6. 重いQueryコールのリスク（Export API）
*   **Severity:** Medium
*   **File:** /crates/evm-core/src/export.rs
*   **Evidence:** export_blocks関数は max_bytes で制限をかけていますが、内部で state.blob_store.read を繰り返します。
*   **Impact:** BlobStoreの読み出しはStable Memoryへのアクセスを伴うため、大量のブロックを一括エクスポートしようとするとQueryの命令数制限に引っかかる可能性があります。
*   **Fix:** ページネーションの単位をバイト数だけでなく、ブロック数でも厳格に制限する（例: 最大100ブロックまで）。

---

### 4) アーキテクチャ不変条件と現状の破りポイント

| 守るべきルール（Invariant） | 現状の違反（Violation） |
| :--- | :--- |
| **O(1) / O(log N) Execution** <br> ブロック生成コストは総データ量に依存してはならない。 | **違反 (Critical):** compute_state_root が全アカウント・全ストレージを走査している。rekey が全Mempoolを走査している。 |
| **Recoverability** <br> 永続データに不正なバイト列があっても、読み出し処理でCanisterを殺してはならない。 | **違反 (High):** from_bytes 実装の至る所で ic_cdk::trap が埋め込まれている。 |
| **Stable Memory Safety** <br> メモリ領域（MemoryId）の重複使用や未初期化アクセスがあってはならない。 | **順守:** MemoryManager と AppMemoryId enum (Source 140) により、ID管理は体系的に行われている。ここは良好。 |
| **Determinism** <br> 同じ入力からは必ず同じ結果（Block/State）が得られなければならない。 | **順守:** OverlayMap (Source 36) でBTreeMapを使用し、Commit順序をキー昇順に固定している点は評価できる。 |

---

### 5) 修正の優先順ロードマップ

リスクを最小化し、システムを稼働可能な状態にするための手順です。

1.  **【最優先】State Root計算のダミー化または差分計算化**
    *   現状の全件走査実装を削除し、一旦は「トランザクションハッシュのハッシュ」等をState Rootとして代用するか、変更セット（Overlay）のみからハッシュを計算する簡易実装に切り替える。これをしないと、データが増えた瞬間に死にます。

2.  **【優先】Mempool Rekeyロジックの修正**
    *   rekey_ready_queue_with_drop での全件ループを廃止する。ready_queue の構造を見直し、再計算が必要なTxのみを処理するか、処理数に上限を設ける。

3.  **【重要】Storable 実装の trap 除去**
    *   from_bytes 内の ic_cdk::trap を panic! に変えるのではなく、データの不整合を検知してもデフォルト値を返すか、エラーログを出して処理を継続できる（あるいはそのエントリを無視できる）構造にする。

4.  **【推奨】RNGのスタブ化解除**
    *   evm-rpc-e2e 等のテストだけでなく、本番ビルドでも getrandom がエラーにならないよう、IC環境向けのRNGシムを導入する。

### 6) 追加で必要な情報（要求事項）

1.  **依存クレート revm のバージョンとパッチ詳細**
    *   Cargo.toml に path = "../../vendor/revm" とあるが、オリジナルの revm に対してどのような変更を加えているか（特に no_std 対応やIC固有の変更）の情報が必要です。
2.  **Cycle ガバナンス設定**
    *   produce_block は誰が呼び出すのか？ タイマー(ic-cdk-timers)で自動化されているようだが、Cycle枯渇時の挙動や、Canisterの凍結防止策（Cycle Threshold等）の設定ファイルまたはコードが見当たりません。
3.  **Upgrade時のデータ移行計画**
    *   Meta 構造体でバージョン管理しているが、実際にレイアウト変更が必要になった際のマイグレーションロジック（V1 -> V2変換コード）が現状存在しないため、将来の拡張計画が必要です。

監査したらこんな感じでした・・・
思考時間: 34s

この監査メモ、かなり筋がいいです。特に **「IC上で死ぬ典型パターン」**をちゃんと踏んで指摘してる。

ただし前提を1個だけハッキリさせると、これは「セキュリティ監査」というより **“計算量・復旧性・運用性の信頼性監査”**で、PoCでも刺さるブロッカーが混ざってます。

以下、俺の評価（同意/補足/反論できる所）と、今やるべき実装タスクに落とします。

まず結論：Critical 3点は「放置すると確実に死ぬ」系
1) State root が全走査（Critical）

これは 本当にヤバい。ICは「メッセージ内の命令数・時間」に上限があるので、状態が育った時点で produce_block が永久に落ちるタイプの時限爆弾になる。

監査メモの通り、accounts/storage/codes を全部イテレートしてるならアウト。

“Merkle/Verkleを実装しろ”は正論だけど重い。Phase1の現実解は次。

Phase1の実装案（現実路線）

state_root を 厳密MPT互換としては扱わない（Phase2以降に先送り）。

代わりに「決定的で、差分だけで更新できる root」を入れる。

例：state_root = keccak(prev_state_root || block_number || tx_list_hash || overlay_change_hash)

overlay_change_hash は commit 対象の (key,value) をキー順にハッシュ連結して作る（OverlayMapならやりやすい）

これで O(changes) にできる。

これをやらない限り、どれだけ pruning/indexer を整備しても本体が育つと止まる。

2) base_fee変動で ready_queue 全件 rekey（Critical）

これも 設計として破綻しやすい。EIP-1559絡みで ordering を “正確に”やろうとすると、base_fee が動くたびに effective_gas_price が変わる＝並び替えたくなる、になりがち。でも全件 rekey は IC だと死ぬ。

現実解（どれかに割り切る）

A案：キーを base_fee 非依存にする

ReadyKey を (max_fee_per_gas, max_priority_fee_per_gas, seq) みたいに固定しておいて

produce_block で 先頭から最大K件だけ取り出して “その時点の base_fee で effective を計算して選ぶ”

“厳密な mempool 並び”ではないが、ICで動く。

B案：rekeyを分割実行（tick化）

rekeyを1回で全件やらず、max_ops_per_tick で少しずつ進める

ただし ordering の一貫性が微妙になるので、決定性の設計が必要

C案：Phase1は FIFO に戻す

これはあなたが「FIFOに戻らない」方針だったので、多分採らないと思う

監査の「全件走査をやめろ」は正しくて、どの割り切りを選ぶかの問題。

3) Storable::from_bytes に trap 多用（High）

これもその通りで、壊れた1レコードで canister が半永久的に死ぬパターン。

ただしここは Rust 的に厄介で、Storable::from_bytes が Result を返せない設計なので、きれいに直すには方針が要る。

Phase1の現実解

from_bytes では trap しない

不正バイト列は “破損レコード”として丸める

例：Receipt { status: 0, gas_used: 0, return_data: empty, logs: empty, corrupted: true }

そして API 返却時に corrupted を見て、必要なら “DecodeError” 的に扱う（ただし trap はしない）

もう一段安全にするなら、

Stableには「長さ＋生bytes」だけを置き

decode は export/query 側で Result で行う（壊れてても読み出しはできる）

監査の “trap禁止” はガチで重要。ここは「セキュリティ」じゃなく リカバリ不能性が問題。

Medium 指摘は「当たってるが優先度は落ちる」
RNGスタブ（wasm getrandom unsupported）

これは「署名を“生成”する経路が入ったら死ぬ」。
あなたの用途が “検証/リカバーのみ”なら当面は致命傷じゃないが、依存がいつどこで rand を踏むかが怖い。

最小対策

alloy-signer* など “署名生成”に関わる経路を canister 側から排除（featureで切る）

どうしても必要なら raw_rand（updateでのみ可）で PRNG を作る

Export API の重さ

あなたはすでに Chunk + max_bytes + cursor に寄せてるので方向性は良い。
追加するなら “ブロック数上限（max_blocks）” を入れるのは保険になる。

じゃあ「次に何するべきか」：実装タスクに落とす

監査メモを踏まえるなら、次の順番が最短で堅いです。

Task 1（最優先）: state_root を O(changes) に置き換え

既存の全走査 root 計算は 消す or デバッグ用途に隔離

overlay の変更セットから決定的 hash を作る（キー順）

Task 2: mempool ordering の “全件rekey” を撤廃

“先頭K件だけ評価”方式か、tick化か、割り切りを選ぶ

ここはチェーン停止リスク直結

Task 3: from_bytes trap を根絶

全 Storable 実装を “破損でも返す”に統一

「破損検知」用のフラグ or sentinel を入れる

Task 4（運用前に）: export/query の hard limit を二重化

max_bytes に加えて max_blocks 的な上限

返却サイズ（2MB）の都合があるので、**“サーバ側で絶対に超えない”**を保証する