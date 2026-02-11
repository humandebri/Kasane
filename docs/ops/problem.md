Executive Summary
対象システムは、Internet Computer (IC) 上で稼働するEVM（Ethereum Virtual Machine）の実装です。Rustによるrevmの統合、Stable Memoryを利用した永続化層（evm-db）、およびJSON-RPC互換のAPI層で構成されています。 監査の結果、極めて重大なセキュリティ欠陥（Critical）MemPoolへの無制限な投入によるDoSリスクが存在し、運用継続性に深刻な影響を与える可能性があります。これらは即時の修正が必要です。

--------------------------------------------------------------------------------
重大指摘 (Critical/High)
1. 管理者機能へのアクセス制御欠落 (Critical)
• Evidence: ファイル /crates/ic-evm-wrapper/src/lib.rs において、以下のUpdateメソッドは ic_cdk::api::is_controller による呼び出し元検証を行っていません。
    ◦ set_auto_mine
    ◦ set_mining_interval_ms
    ◦ set_prune_policy
    ◦ set_pruning_enabled これにより、管理者系メソッド間でアクセス制御実装が不統一になっています。
• Impact: 認証なしで誰でも以下の操作が可能です。
    ◦ マイニング（ブロック生成）の停止 (set_auto_mine(false))。
    ◦ データ削除ポリシーの無効化 (set_pruning_enabled(false)) によるストレージ枯渇の誘発。
    ◦ マイニング間隔の極端な変更による動作不安定化。
• Fix: 該当する全ての関数に以下のガード句を追加してください。
2. ドロップされたトランザクションのメモリリーク (High)
• Evidence:
    ◦ produce_block 内で実行不能（デコードエラー、ガス不足等）と判断されたTxは TxLoc::dropped としてマークされ、TxStore（本文）と TxLocs（位置情報）に残ります。
    ◦ データ削除を行う prune_blocks は、state.blocks に存在するブロック番号に基づいてループ処理を行っています。
    ◦ ドロップされたTxはブロックに含まれないため、ブロック番号と紐付かず、削除ループの対象外となります。
• Impact: 無効なトランザクションやガス不足で失敗したトランザクションのデータが StableBTreeMap (TxStore) 内に永続的に蓄積されます。これによりStable Memoryが徐々に圧迫され、最終的にキャニスターがメモリ上限に達し停止します（Zombie state）。
• Fix: ドロップされたTxを定期的に削除する仕組みを導入してください。
    ◦ 案1: produce_block でドロップ判定した時点で即座に TxStore から削除する（現状は TxLoc 更新のみ）。
    ◦ 案2: ドロップ時間を記録し、別途タイムスタンプベースでクリーンアップするタスクを追加する。
3. MemPoolへの無制限な投入によるDoS (High)
• Evidence:
    ◦ submit_tx および submit_ic_tx は、トランザクションサイズ (MAX_TX_SIZE) のチェックは行いますが、送信者ごとの保留Tx数やシステム全体のキューサイズを制限していません。
    ◦ 残高チェックやNonceの整合性チェックは、ブロック生成時 (produce_block -> select_ready_candidates) または実行時まで遅延される場合があります。
    ◦ submit_tx 内での min_fee_satisfied はTxのガスプライス設定を見るだけで、送信者の残高から手数料を徴収しません（EVMの仕様上、実行時に徴収）。
• Impact: 攻撃者は少額のICサイクルコストで大量の無効または未来のNonceを持つトランザクションを送信し、TxStore と Pending キューを溢れさせることができます。これにより正当な利用者のTxが処理されなくなるか、ストレージ枯渇を引き起こします。
• Fix:
    ◦ submit_tx 時に state.pending_by_sender_nonce の数をチェックし、送信者ごとの上限（例: 64件）を設ける。
    ◦ システム全体の ready_queue または seen_tx の総数にハードキャップを設ける。

--------------------------------------------------------------------------------
中〜軽微指摘 (Medium/Low)
4. Queryメソッド get_queue_snapshot のループ制限不備 (Medium)
• Evidence: /crates/ic-evm-wrapper/src/lib.rs の get_queue_snapshot は、引数 limit をユーザー入力から受け取り、その回数分だけ state.ready_queue をイテレートします。上限値の定数定義によるガードがありません。
• Impact: 極端に大きな limit を指定することで、クエリ呼び出しがインストラクション制限に達し、常にエラーとなる可能性があります（APIの可用性低下）。
• Fix: limit に対してサーバーサイドでのハードキャップ（例: 1000件）を適用してください。
5. Pruning Journal の冗長性と複雑性 (Low)
• Evidence: /crates/evm-core/src/chain.rs の prune_blocks は、削除対象のBlobポインタを PruneJournal に保存してから削除し、最後にJournalを消しています。
• Impact: ICのアーキテクチャ上、1つのメッセージ実行（update call）がトラップ（パニック）すると、その実行中の状態変更は全てロールバックされます。したがって、処理途中のクラッシュ対策としての自前ジャーナリングは不要であり、無駄な書き込みコスト（サイクル消費）が発生しています。
• Fix: ic-stable-structures のアトミック性を信頼し、Journal処理を削除してコードを簡素化・高速化することを推奨します。

--------------------------------------------------------------------------------
アーキテクチャ不変条件と現状の破りポイント
不変条件 (Invariant)
現状の違反 (Violation)
管理者権限の保護
set_auto_mine 等の重要な設定変更メソッドがパブリックアクセス可能である。
ストレージの有限性
ドロップされたTxが削除されず、永続的にストレージを消費し続ける（Unbounded growth）。
リソースの公平性
1人のユーザーがMemPoolを無制限に占有可能であり、他者の利用を阻害できる。
状態の整合性
BlobStore の TxIndex などは with_state_mut 内で個別に追加されるが、親となる TxStore との整合性はコード上のロジックに依存しており、データベース的な外部キー制約はない（コードレベルでの担保が必要だが、上記リークにより整合性が崩れている）。

--------------------------------------------------------------------------------
修正の優先順ロードマップ
1. Phase 1: セキュリティホールの即時塞ぎ (Emergency Patch)
    ◦ ic-evm-wrapper/src/lib.rs: 全ての update メソッド（rpc_eth_send_raw_transaction などを除く管理系メソッド）に is_controller チェックを追加する。
    ◦ これにより、第三者によるシステム停止を防ぐ。
2. Phase 2: メモリリークの解消 (Stability)
    ◦ evm-core/src/chain.rs: produce_block 内でTxをドロップする際、state.tx_store.remove(tx_id) および state.tx_locs.remove(tx_id) を即時実行するように変更する。または TxLoc::Dropped に timestamp を持たせ、定期タスクで削除する。
3. Phase 3: DoS対策 (Reliability)
    ◦ evm-core/src/chain.rs: submit_tx に以下のチェックを追加。
        ▪ Global Pending Limit: 全体の未処理Tx数が上限なら拒否。
        ▪ Per-Sender Limit: 同一 sender の保留Tx数が上限なら拒否。
4. Phase 4: 最適化
    ◦ 不要な PruneJournal の削除。
    ◦ get_queue_snapshot の limit キャップ追加。

---------------------------------
