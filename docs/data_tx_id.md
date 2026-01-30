1) tx_id の定義（これが最優先の根っこ）
Route A: 署名付き raw Ethereum Tx

tx_id = Ethereum tx hash と一致させる。

tx_id = keccak256(raw_tx_bytes)

これは Ethereum の定義どおり（RLP/typed tx を含めて「署名込みの生バイト列」を keccak）

これで Phase2 の eth_getTransactionByHash がそのまま通る。

Route B: IC 合成 Tx（canister 呼び出し由来）

Ethereum互換ハッシュは作れない（署名が無い/形式が違う）ので、独自 tx_id を定義する。ただし衝突耐性と将来拡張のため domain separation を必ず入れる。

tx_id = keccak256( domain_sep || version || chain_id || canister_id || caller_principal || caller_nonce || payload_hash )

具体：

domain_sep = b"icp-evm:synthetic-tx"

version = 0x01（1 byte）

chain_id: u64（固定値）

canister_id: Principal（このEVM canisterのprincipal。bytesで）

caller_principal: Principal（呼び出し元）

caller_nonce: u64（callerごとの単調増加 nonce。必須）

payload_hash = keccak256(payload_cbor_or_candid_bytes)（payloadそのものは入れずhashで十分）

caller_nonce の扱い

nonce は canister 側が保持し、submit_ic_tx 時に nonce += 1 して採番

外部から nonce 指定を許すなら検証が要るので、Phase1では canister採番固定でいい

これで「同じcallerが同じpayloadを2回投げてもtx_idが別になる」。リトライも安全。

2) stable schema（Phase1で固める“最低限 + 将来の逃げ道”）

設計方針：

安定領域に巨大Vecをそのまま置かない（アップグレード/メモリ移動が痛い）

stable-structures の StableBTreeMap 前提で「キー→値」ストアに分解

先頭に SchemaVersion と Config を置く

ブロック/receipt/txloc を別マップにする（索引は最小）

2.1 Top-level: StableState（1つのルート）
/// stable root (versioned)
struct StableStateV1 {
  // schema
  schema_version: u32, // = 1

  // chain config
  chain_id: u64,
  canister_id: [u8; 29], // Principal bytes (variableだが固定長にパックしても良い)
  auto_mine_enabled: bool,
  max_txs_per_block: u32,
  block_gas_limit: u64,

  // deterministic time model
  last_block_time: u64,      // seconds
  last_block_number: u64,    // tip

  // re-entrancy / heartbeat guard
  is_producing: bool,

  // queue bookkeeping
  next_queue_seq: u64,       // monotonic enqueue id
}


これは “小さい固定領域” なので stable に直置きしてOK。

2.2 Queue（mempool無しの中核）
QueueItem
enum TxKind { RawEth = 0, ICSynthetic = 1 }

struct QueuedTx {
  tx_id: [u8; 32],
  kind: TxKind,
  seq: u64,          // enqueue order (monotonic)
  // optional minimal payload reference:
  // raw bytes / synthetic payload can be stored elsewhere if needed.
}

stable maps

queue_by_seq: StableBTreeMap<u64, QueuedTx>

key = seq

queue_head_seq: u64 / queue_tail_seq: u64 は StableStateV1 に持つか、next_queue_seq から導く

実装は「head を別に持つ」のが楽（popがO(1)）

ポイント

“キュー本体”は seq で並ぶ。limit/offset も cursor_seq もやりやすい。

2.3 tx_index（pending可視化のための最小索引）
enum TxLocV1 {
  Queued { seq: u64 },
  Included { block_number: u64, tx_index: u32 },
  Dropped { code: u16 }, // optional but recommended
}


tx_loc: StableBTreeMap<[u8;32], TxLocV1>

ルール

submit した瞬間に tx_loc[tx_id] = Queued{seq}

ブロックに入れた瞬間に Included{...} に更新

キューから捨てたら Dropped{...}（OOM/invalidなど）

2.4 Blocks（ブロックヘッダ + tx_id列）
BlockHeaderV1（最小）
struct BlockHeaderV1 {
  number: u64,
  parent_hash: [u8; 32],
  block_hash: [u8; 32],
  timestamp: u64,
  state_root: [u8; 32],
  tx_list_hash: [u8; 32], // keccak256(concat(tx_id...)) でもよい
  gas_used: u64,
}

BlockBodyV1
struct BlockBodyV1 {
  tx_ids: Vec<[u8;32]>, // size <= max_txs_per_block
}

stable maps

blocks_header: StableBTreeMap<u64, BlockHeaderV1>

blocks_body: StableBTreeMap<u64, BlockBodyV1>

ヘッダとボディを分けると、ヘッダだけ読む用途（Phase2 RPC）で軽くなる。

2.5 Receipts（logs保存：索引なし）
ReceiptV1（最小互換）
struct LogV1 {
  address: [u8; 20],
  topics: Vec<[u8; 32]>,
  data: Vec<u8>,
}

struct ReceiptV1 {
  tx_id: [u8; 32],
  block_number: u64,
  tx_index: u32,
  status: u8,                 // 1 or 0
  gas_used: u64,
  cumulative_gas_used: u64,   // optionalだが互換が上がる
  return_data: Vec<u8>,       // revert含む raw bytes
  contract_address: Option<[u8;20]>,
  logs: Vec<LogV1>,
}

stable map

receipts: StableBTreeMap<[u8;32], ReceiptV1>

索引なしポリシーでも、

block -> tx_ids -> receipts[tx_id]
で全部取れる。

2.6 実行State（EVM state DB）

ここはあなたの REVM fork 側の設計に依存するけど、stable schema としては “stateのコミットを別領域” にするのが重要。

最小の逃げ道：

state_snapshot_root_by_block: StableBTreeMap<u64, [u8;32]>

state_root と一致させる（ヘッダにも入ってるが別に持つのは将来用）

実際のKV（accounts/storage）は Phase1のStateDB設計で決めるとして、ブロック単位コミット境界が stable に残ることだけ保証する。

3) ハッシュ類の定義（仕様として固定）

raw tx_id：keccak256(raw_tx_bytes)

tx_list_hash：keccak256( concat(tx_id_0 || tx_id_1 || ... ) )

block_hash：Phase1は簡略でもいいが、後で互換を取りたければ

block_hash = keccak256( encode(header fields in a fixed canonical encoding) )

ここで encoding は「自作の固定バイト列」でも良い（RLP互換を狙わないなら）

state_root：StateDBのコミット結果（REVM forkで決定的に）

Phase2でeth互換ヘッダhashに寄せたくなった時のために、block_hash の “計算方式version” をヘッダに1byte入れてもいい。

4) Upgrade（壊れないための最低条件）

ルートは StableState = enum { V1(StableStateV1), V2(...) } みたいにバージョン付き

post_upgrade で is_producing=false に強制リセット（安全側）

追加フィールドは “末尾に足す” or “V2を作る” のどちらかに固定

5) これで実現できるAPI（即）

submit_raw_tx(raw) -> tx_id

submit_ic_tx(payload) -> tx_id

get_pending(tx_id) -> TxLocV1 or Unknown

get_queue_snapshot(limit, cursor_seq) -> items + next_cursor

get_block_by_number(n) -> header + tx_ids

get_receipt(tx_id) -> ReceiptV1

Phase2のRPCはただの翻訳機になる。
