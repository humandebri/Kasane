Phase1 Spec + 実装計画（REVM fork 前提）
Phase1の目的

@vendor/revm/examples/my_evm を参考にすること

Route A（Eth署名raw tx）/ Route B（IC合成Tx）の 実行

FIFO順に ブロック化（produce_block or execute_*）

REVMで順に 実行→state更新→stable永続化

ブロックごとに tx_list_hash / state_root / block_hash を保存

同じtx列なら同じroot（決定性）＋ upgrade後も壊れない

非目的

HTTP RPC（Phase2）

mempool / pending（不要）

OP/ZK（Phase3+）

logsの完全互換（receiptは最小でOK）

1. 外部API（canister）
1.1 update（submit中心に統一する）

Phase1でどっちを採るかを明確化しておくのが重要です。

A案（採用）：submit_* + produce_block だけ（同期即時レーンを廃止）

submit_eth_tx(raw_tx) -> tx_id
submit_ic_tx(tx_bytes) -> tx_id
produce_block(max_txs) -> ProduceBlockStatus

submit_ic_tx(...) -> tx_id

produce_block(max_txs) -> block_number

どのみちPhase2以降で “SDKから叩ける” が必要になるので、個人的にはA案で良い。

1.2 query

get_block(n) -> BlockView

get_receipt(tx_id) -> ReceiptLike

eth_call_like(...) -> return_data（overlayで実行、永続変更なし）

2. Txモデル（凍結・決定性に直結）
2.1 TxEnvelope（stableで保存する最小単位）
TxEnvelope {
  tx_id: [u8;32],      // keccak256(bytes)
  kind: {EthSigned, ICSynthetic},
  tx_bytes: Vec<u8>,   // raw_tx or ic_tx_bytes（Phase0で凍結した形式）
}


tx_id = keccak256(tx_bytes) を推奨（idempotent・参照が楽）

seen_tx で重複排除（stable）

2.2 Route B（IC合成Tx）のfrom/nonce方針（Phase1で決める）

おすすめ：nonceは “EVMアカウントnonce” に統一する

from = caller_evm_from_principal(caller)（Phase0凍結）

stateのaccount.nonceをREVMが更新（Ethと同じ）

nonce? 指定があるなら 一致しなければ reject（同期API向け）

※ 以前出てた nonce_ic: Map<principal,u64> は不要にできる。簡単で壊れにくい。

2.3 ICSynthetic bytes（Phase1の暫定フォーマット）
version: u8 (=2)
to: [u8;20]
value: [u8;32] (big-endian)
gas_limit: u64 (big-endian)
nonce: u64 (big-endian)
max_fee_per_gas: u128 (big-endian)
max_priority_fee_per_gas: u128 (big-endian)
data_len: u32 (big-endian)
data: [u8; data_len]
chain_id: 4801360 (0x494350, "ICP") をTxEnv/CHAINIDに固定

caller は "ic-evm:caller_evm:v1" || principal_bytes の keccak256 末尾20 bytes（Phase1暫定）

注記
- Eth raw tx のデコードは未実装（Phase1の次段で追加予定）

3. ブロックモデル（Phase1で固定）
3.1 BlockData（stable）
BlockData {
  number: u64,
  parent_hash: [u8;32],
  block_hash: [u8;32],
  timestamp: u64,          // deterministic rule
  tx_ids: Vec<[u8;32]>,    // 実体bytesは tx_store から引ける
  tx_list_hash: [u8;32],
  state_root: [u8;32],
}

3.2 timestampルール（決定性）

ICの time() を使うと実行環境の差で揉めるので、Phase1では 完全決定的にするのが安全。

timestamp = parent.timestamp + 1（または +固定Δ）

genesisは timestamp = 0（固定）

これで再実行一致が保証される。

3.3 tx_list_hash（凍結）

POCはこれで十分（固定長連結、曖昧性なし）：

tx_list_hash = keccak256( 0x00 || tx_id1 || tx_id2 || ... || tx_idN )

（N=0なら keccak256(0x00) など固定）

3.4 block_hash（凍結）

block_hash = keccak256( 0x01 || parent_hash || u64_be(number) || u64_be(timestamp) || tx_list_hash || state_root )

※ RLPでもいいが、固定長連結が事故りにくい。

4. 永続State（Phase0のStableBTreeMapをEVM状態にする）

Phase0で確定した3つのstable mapを “EVMのStateDB” として扱う。

accounts: AccountKey -> AccountVal

storage: StorageKey -> U256Val

codes: CodeKey -> CodeVal

4.1 REVMフォーク側（最重要：DB adapter + commit）
(1) Database adapter

REVMが読むのは get_basic / get_code_by_hash / storage など

これを StableState + Overlay に繋ぐ

(2) Committer（決定性の核）

REVMの実行結果（state diff）を Overlayへ書き込み → commit() で stable map へ反映

Overlayのwritesは BTreeMap（昇順保証）

commit() は writes.iter() を順に適用（Phase0の仕様どおり）

4.2 Tombstone/正規化（EVM storage 0の扱い）

EVMでは「slotが0」は通常 “未保存” と等価。
Phase1から入れるのがおすすめ：

storage書き込みで value == 0 なら delete(slot) に正規化
（stateサイズが増えない・後のroot計算も軽い）

4.3 SELFDESTRUCT の扱い（POCでも要注意）

正しくやると「そのアドレスのstorage全消し」が必要。StableBTreeMapはprefix scanできるが重い。

Phase1の現実案：

account削除（AccountKey remove）

code参照解除（code_hashが残ってもOK、コード本体はGCしない）

storageは prefix scan で全削除（addr20の範囲をrangeで走査して削除）

これは重いので Phase1では DoS制限とセットで許容

5. state_root（POC commitment）実装（Phase1で実装）

Phase0で root 規則（leaf hash規則・内部hash・空root）を凍結した前提で実装。

5.1 “全件再計算”でいい（まずは）

Phase1は性能より正しさ。最初は各ブロック確定時に:

accounts（キー順）→ storage（キー順）→ codes（キー順）を順に走査

それぞれ leaf_hash(key||value) をストリームして MerkleRoot 計算

キーprefixが 0x01/0x02/0x03 なので、全体の辞書順は “accounts→storage→codes” の連結で一致します（別途マージソート不要）。

※ touched-set を使った差分rootは Phase1後半/Phase2で最適化すればいい。

6. Receipt（Phase1最小）

RPC前でも必要。最低限この形を保存。

ReceiptLike {
  tx_id: [u8;32],
  block_number: u64,
  tx_index: u32,
  status: u8,           // 1 success, 0 revert/fail
  gas_used: u64,
  return_data_hash: [u8;32],  // return_dataが大きいとき
  contract_address?: [u8;20], // create時
}


logsはPhase1では空で良い（互換を本気で取るのはPhase3以降で沼）。

7. Phase1 stable構造（追加分）

Phase0に加えてPhase1で増えるもの：

queue: VecDeque<TxEnvelope>（submit_* 用）

seen_tx: Set<[u8;32]>

tx_store: Map<tx_id -> TxEnvelope>（後から参照するため必須）

tx_index: Map<tx_id -> {block, pos}>

receipts: Map<tx_id -> ReceiptLike>

blocks: Map<u64 -> BlockData>

head: Cell<Head>（tip番号/parent_hash/timestamp 等）

これらはPhase0のMemoryIdを末尾追加で割り当てる（or 予約済みに刺す）。

8. エラー規約（Phase1で固定）

decode不能（Eth raw tx / ICSynthetic bytes不正）：reject（updateでtrapではなくResult推奨）

nonce mismatch（execute系でnonce指定時）：reject

queue full / tx too large / gas too large：reject

REVM実行revert：ブロックに含める＋status=0（EVM的）

9. 実装タスク分解（そのままチケットにできる粒度）
9.1 状態層（Phase0の上）

 StableState（accounts/storage/codes）の実体化（Phase0でできてる前提）

 Overlay を汎用化（Key/Val別）

 storage 0 正規化フック

9.2 REVM fork（核）

 StableDbAdapter（REVM Database trait実装）

 Committer（REVM diff → overlay → stable commit）

 SELFDESTRUCT対応（prefix scan削除）

9.3 チェーン層

 TxEnvelope と tx_id 生成

 BlockBuilder（timestampルール、tx_list_hash、block_hash）

 state_root計算（全件走査版）

9.4 canister API

 submit_* + produce_block（唯一の標準書き込み導線）

 query get_block/get_receipt/eth_call_like

9.5 upgrade

 Phase0のmeta検証を必ず通す

 head/blocks/tx_store等の再初期化（StableBTreeMap::init）

10. Phase1の合格条件（テスト）

最低でもこれが通れば「捨てないPOC」になる。

決定性

同じtx列（Eth/Ic混在）を2回流して最終 state_root 一致

ブロックごとの block_hash も一致

upgrade耐性

upgrade後も head/blocks/tx_store/state が壊れない

同じ入力に対して同じroot

EVM基本動作

contract deploy（CREATE）→ call → storage更新 → revertケース

SELFDESTRUCT の後に storageが消えている（最低限）

DoS制限

max_tx_size / max_gas_per_tx / max_code_size が効く

11. 最低限のスモーク手順（canister）

目的: canister上での落ち方/起動を確認し、stable上の壊れたtxを巻き込んで落ちないことを保証する。

必須:
- cargo test -p ic-evm-wrapper

手動スモーク（dfx）:
- dfx start --clean --background
- source scripts/lib_init_args.sh && INIT_ARGS="$(build_init_args_for_current_identity 1000000000000000000)"
- dfx canister install <canister> --mode reinstall --wasm target/wasm32-unknown-unknown/release/ic_evm_wrapper.candid.wasm --argument "$INIT_ARGS"
- dfx canister call <canister> dev_mint '(vec {0x11: nat8; ...}, 1000000000000:nat)' ※任意（controllerのみ）
- dfx canister call <canister> submit_tx '(blob "<raw_eip2718_tx>")'
- dfx canister call <canister> produce_block '(1:nat)'
- dfx canister call <canister> get_block '(0:nat)' / get_receipt

旧データ混入の起動確認（最低1回）:
- 旧バージョンで tx_store に不整合を作る
- upgrade して起動
- その後の query/update が落ちず、該当txが drop_code=decode で処理されることを確認

実装メモ（現状）
- MAX_TX_SIZE = 128KB（Phase1のPoC上限）
- MAX_TXS_PER_BLOCK = 1024
- tx_list_hash/block_hash は spec通りの keccak 連結で実装済み
- StableDbAdapter / Committer / SELFDESTRUCT / RevmStableDb の骨組みまで実装済み（REVM実行統合は次の段階）
