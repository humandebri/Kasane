全体戦略（方針を一本化）

コアは “単一canister内で同期Txっぽく完結するEVM”（= ICPから呼んで嬉しい）

RPCは「ノード互換」より “開発・デバッグ・外部ツール接続”のための面としてPhase2で入れる

L2（L1担保）系は、いきなりOP/ZKに行かず アンカー→trusted→（必要なら）OP の順で硬くする

Phase 0: 仕様凍結（手戻り防止フェーズ）

目的：決定性が壊れる種を先に潰して凍結。

凍結するもの（最重要）

caller_evm 導出ルール（"ic-evm:caller_evm:v1" || principal_bytes の keccak 末尾20byte）

ICSyntheticのcanonical encoding（CBORならcanonical CBORを明記、個人的にはRLP/TLV推奨）

StateDB keyspace（例：0x01||addr / 0x02||addr||slot / 0x03||code_hash）

Merkle規則（奇数葉、空root、連結順、hash関数）

EVM環境値の固定セット（chain_id/basefee/gas_limit/timestampルール）

成果物

spec.md（凍結事項だけ書いた短い仕様）

golden vectors（principal→addr, ic_tx_bytes→hash）

Phase 1: “同期Tx体験”が核の実行基盤（REVMフォーク＋stable）

目的：ICPから呼んで「その場で結果が分かる」EVMを作る。RPCより先。

1.1 StableDB + OverlayDB + Commit（心臓）

ic-stable-structuresで永続KV

Overlay（RAM差分）→ commit（順序固定）

stable state versioning（upgrade耐性）

1.2 実行API（submit中心）

同期即時実行レーンは廃止し、書き込みは submit_* + produce_block に統一する。

update submit_ic_tx(...) -> tx_id
update submit_eth_tx(raw_tx) -> tx_id
update produce_block(max_txs) -> ProduceBlockStatus

※ 同期実行APIは提供しない。書き込みは `submit_* + produce_block` に統一する。

1.3 最小のブロック/Tx/Receipt保存

RPC前でも必要：

tx_envelope_store

tx_index

receipt_store（logs空でも良い）

block_hash 定義

Phase1の合格条件

同一tx列→同一state_root（再現）

upgrade後も壊れない

execute_* で success/revert と return_data が取れる

Phase 2: RPCノード化（HTTP JSON-RPC受付）

目的：外部ツール（viem/ethers/foundry）を繋ぎ、開発者体験を作る。

2.1 HTTPルーティング

http_request（query）：読み取り系

http_request_update（update）：eth_sendRawTransaction 等

2.2 実装優先順位（“最低限ノード”）

eth_chainId, eth_blockNumber

eth_getBlockByNumber, eth_getTransactionReceipt, eth_getTransactionByHash

eth_call, eth_getBalance, eth_getCode, eth_getStorageAt

eth_sendRawTransaction（update）

注意（仕様の割り切り）

pending/mempoolはやらない（latestのみ）

logs/filter（eth_getLogs）はPhase2ではやらない（沼回避）

Phase2の合格条件

viem/ethersで接続でき、deploy/call/sendRawTxが通る

レスポンスが決定的（同状態→同応答）

Phase 2.5: “ICPから呼べる価値”をプロダクト形にする（差別化フェーズ）

目的：「ICから呼べるだけ」を「これ作れる」に変える。チェーンの看板を作る。

3つの看板候補（どれか1つ選んで最短実装）

IIガスレス標準：IIログイン→ERC20/NFT操作がボタンで完了

ワークフロー×台帳：申請/審査/承認をICPで管理し、確定だけEVMに書くテンプレ

自動化（bot不要）：canisterが条件成立で自動Tx（配布/清算/期限処理）

成果物

サンプルdapp（フロント＋ICP canister＋EVM contract）

SDK（TSで submit_ic_tx + produce_block を叩く薄いクライアント）

Phase 3: L1アンカー＋trusted bridge（L2“体験”フェーズ）

目的：外部EVMと価値を接続する。ただしトラストレスは後回しでOK。

3.1 L1アンカー（掲示板）

L1に block_hash/tx_list_hash/state_root を投稿するコントラクト

proposerはまず固定（運営/DAO）

3.2 trusted bridge（資産が動く体験）

relayer/マルチシグでdeposit/withdraw

ガードレール（timelock/停止スイッチ/上限）

Phase3の合格条件

L1↔あなたのチェーン間で資産が動く（trust前提でOK）

監視可能（アンカーにより透明性がある）

Phase 4: OP（異議申し立て）に行くかどうか（選択フェーズ）

ここは「L1担保を名乗りたいか」で決める。

行くなら：challenge window + dispute game + 最小検証単位の設計（大工事）

行かないなら：DAO/運用ガードを強化して “透明な運営チェーン” として押し出す

工数感（現実）

Phase1（同期Tx核 + stable +決定性テスト）：一番重い

Phase2（RPC）：中

Phase2.5（差別化dapp）：軽〜中（でも重要）

Phase3（アンカー＋trusted bridge）：中〜重（外部要素が増える）

重要な変更点（今回の“練り直し”の芯）

Phase1に “execute_*（即時実行）” を入れて、ICPから呼ぶ価値を最初から勝ちに行く

RPCはPhase2で入れるが、ノード完全互換を狙わず必要十分に切る

L2（L1担保）は アンカー→trusted→OP の順に段階化
