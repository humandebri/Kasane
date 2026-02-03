目的と設計原則
目的

EVM/OP Stack の「挙動（状態遷移・例外・ガス）」を op-revm/revm/alloy に寄せて再発明を削る

IC 固有の部分（入口・永続化・アップグレード耐性）だけを独自実装として残す

Phase4（異議申し立て/検証）で必要な整合性（特に deposit/system tx・L1 cost・state root）を最短距離で満たす

原則（ここブレると地獄）

Semantics（どう動くか）= ライブラリに委譲

op-revm の Handler / Deposit / L1BlockInfo / L1 cost

alloy の Tx/Receipt/Log/EIP計算

trie（state root）は標準実装へ

Storage（どこに残すか）= IC 独自

ic-stable-structures / MemoryId 固定 / upgrade 耐性

Ingress（どう入力を受けるか）= IC 独自

IcSynthetic、Principal→Address、権限、課金（cycles）

全体ロードマップ（PR分割前提）
PR0: “壊さないための土台”を先に作る（必須）

やること

既存実装のまま以下を固定出力できるテスト/スナップショットを追加

tx → (receipt, logs, state_root, gas_used, halt_reason)

ブロック単位：block_hash / state_root / tx_list_hash

“参照実装”との差分が見れる 差分テスト（differential test） を用意

ローカルで op-geth / op-node / reth 等と突合できる形（オフチェーンでOK）

完了条件

既存HEADでテストが安定し、以降のPRが「差分が意図通り」か判断できる

Phase1: トランザクション系統の再発明をやめる
PR1: Tx表現を alloy/op-revm に寄せる（型の整理）

やること

crates/evm-db/src/chain_data/tx.rs の StoredTx / RawTx / FeeFields を整理し、基本はこれに寄せる：

Ethereum系：alloy_consensus::TxEnvelope（EIP-2718/2930/1559/4844 を含む）

Optimism系：DepositTransaction（op-revm 側の型があるならそれ）

IC固有：IcSynthetic は残すが、“最終的にEVMへ入れる形”に変換できるようにする

推奨の統一入口（例）：

enum TxIn { Eth(alloy_consensus::TxEnvelope), OpDeposit(DepositTx), Ic(IcSyntheticTx) }

完了条件

既存の decode/execute 呼び出しが TxIn に統一され、保存構造体が “独自列挙の乱立” になっていない

PR2: デコードをライブラリへ委譲（自前 decode を減らす）

やること

evm-core/src/tx_decode.rs 相当：

Eth系は alloy_rlp + TxEnvelope::decode へ寄せる

Deposit は op-revm の decode/検証ロジックに寄せる（SourceHash等）

自前の「tx type 判定・RLP境界処理」をできるだけ削除

完了条件

主要Tx（legacy/2930/1559/4844 + deposit）が ライブラリdecodeで通る

失敗理由（invalid rlp / bad sig / wrong chain id 等）が一貫して返せる

Phase2: 実行フロー（Handler）を op-revm に寄せる
PR3: “手続き的EVM実行”→ “OpBuilder/OpHandler” へ移行

やること

evm-core/src/revm_exec.rs や chain.rs の手動フローを縮退させる

op-revm が提供する想定の構成（例：OpBuilder / OpEvm / OpHandler）に寄せる

L1BlockInfo System Tx をブロック先頭に注入（op仕様）

L1 Data Fee を handler で徴収（FASTLZ係数等の定数込み）

Deposit failure の halt reason（例：FailedDeposit）を仕様通りに扱う

DB は “注入するだけ” にする（独自実装を守る範囲を固定）

StableDbAdapter を revm::db::State（Cache/Bundle管理）でラップして渡す

commit 時に出てくる差分（BundleState）だけ永続化層に反映

完了条件

既存のブロック生成が op-revm 経由で成立し、以下が一致する

deposit/system tx の扱い

L1 cost が receipt 等に反映される（少なくとも計算・徴収が入ってる）

revert/oom/out-of-gas の分類が op-revm 側の halt に一致

Phase3: state root / base fee / receipt/log を標準実装へ置換
PR4: Base Fee（EIP-1559）を alloy の標準計算へ

やること

evm-core/src/base_fee.rs の compute_next_base_fee を置換

alloy_eips::eip1559 の計算関数へ委譲

自前定数（elasticity等）を削除して、参照元に寄せる

完了条件

ベースフィー遷移が参照実装と一致（テストで担保）

PR5: state_root.rs の独自MPT風ハッシュを捨てて trie 実装へ

やること

evm-core/src/state_root.rs を 標準Trie実装へ切り替え

候補：alloy-trie or reth-trie（どちらでも良いが“参照実装に近い方”）

重要：DBのイテレーション順序・RLP形式が一致しないと root がズレる
→ “実装を捨てる”のが目的なので、自前の leaf_hash などは残さない方針

完了条件

state root が参照実装と一致（ここがズレると Phase4 の土台が崩れる）

PR6: receipts/logs を alloy 型に寄せる（保存だけ独自）

やること

crates/evm-db/src/chain_data/receipt.rs の独自 ReceiptLike / LogEntry を整理

内部表現は alloy_consensus::Receipt / alloy_primitives::Log を採用（またはラップ）

永続化（StableBTreeMap）のための Storable 実装だけ独自に持つ

RPC を作る時も変換が薄くなる

完了条件

receipt/log の JSON-RPC 互換に近い形が維持され、変換コードが激減

Phase4: エラー系統・署名検証の境界を確定（事故が減る）
PR7: エラー/停止理由を op-revm/revm に寄せて分類を固定

やること

OpError / OpHaltReason（あるいは相当）をそのまま上位へ伝播できる形にする

canister 外部API（Reject/Result）には “安定したエラー分類” でマッピング

Deposit failure の特例は必ず保持（OPの合意に関わる）

完了条件

“文字列エラー地獄”が消え、障害解析・互換性検証がしやすくなる

PR8: 署名検証の責務分離（入口 vs EVM内部）

やること

入口（Ingress）で検証するもの

Eth tx の署名（k256等、wasmで安全に動く範囲）

IcSynthetic の認証（Principal由来など）

EVM内部は precompile に委譲

ecrecover 等は revm の precompile を使う
（二重実装しない）

完了条件

署名検証が二重化していない／責務が明確

SIMD（Wasm SIMD）の入れ方：最後に“性能PR”として分離
PR9: SIMD 有効ビルドの導入（互換性リスクを隔離）

やること

wasm32 向けに +simd128 を付けたビルドプロファイルを追加（例：.cargo/config.toml / build script）

暗号系プリコンパイル（bn254/bls12_381/sha2/ripemd/keccak 等）で効くかを計測

SIMDあり/なし両方ビルドできるようにして、環境差で詰まらないようにする

完了条件

correctness（出力一致）が崩れない

ベンチで改善が観測できる（どこが速くなったか説明できる）

Stable Memory（ic-stable-structures）の移行ポリシー（ここも事故りやすい）

原則

既存 MemoryId を壊さない。壊すなら “新MemoryIdを追加してバージョン移行”。

Storable のバイナリ形式変更が必要なら、必ず versioned encoding にする。

完了条件

upgrade テスト（アップグレード前後で state/blocks/receipts が保持）に合格

優先順位（迷ったらこれ）

state root の標準化（PR5）：Phase4・fraud proofの根。ズレたら全部ムダ。

op-revm handler導入（PR3）：L1BlockInfo/L1 cost/deposit の互換性が入る。

Tx/Receipt/Logの型寄せ（PR1/2/6）：後工程（RPCや検証）が楽になる。

SIMD（PR9）：正しさが固まってから。

最終的に残す「独自実装」チェックリスト（残してOKなやつ）

StableDbAdapter（StableBTreeMapでの永続化）

MemoryId固定レイアウト（Freeze）

IcSynthetic（IC入口要件）

canister API / cycles / 権限

indexer/archiver/metrics（OP互換層とは別問題）