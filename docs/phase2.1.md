# Phase2.1（Explorerの作成 / OSS活用）

## 目的

* 運用/監査の可視化を最速で確保
* pruning前提でも履歴を追えるようにする

## 前提

* canisterは pruning する
* 履歴は **indexerに外出し**する
* WebSocketは後回し（HTTP JSON-RPCのポーリングで成立させる）

---

## OSS候補（用途別）

### 1) Blockscout（UI + API + Indexer全部入り）

* **一番ラク**。Postgres前提。
* RPCエンドポイントを指定すると自動同期して Explorer が立つ。
* WebSocketは推奨だが **無くても動く**（eth_blockNumberのポーリング）。

向いてる:

* とにかく早く “それっぽい Explorer” を公開したい

向いてない:

* RPC互換がまだ薄い/trace系を用意したくない

---

### 2) Otterscan（軽いUIだが前提が重い）

* UI自体は軽いが **Erigonアーカイブノード前提**。
* ICP上の独自EVMとは相性が悪い。

結論: Phase2.1では **非推奨**。

---

### 3) Shovel（Ethereum → Postgres の宣言的インデクサ）

* RPCからブロック/tx/イベントを吸って Postgres へ。
* 宣言的設定で速い。

向いてる:

* indexerだけOSSで済ませて、UIを最小自作する

---

### 4) Ponder（イベント中心のindexer + API生成）

* イベント中心に型付きで index。
* GraphQL/APIが作りやすい。

向いてる:

* **Bridgeイベント中心**に可視化したい

---

### 5) Subsquid（強力だが重め）

* EVMテンプレあり。
* 将来大きくするなら強い。

Phase2.1の最短には **やや重い**。

---

## 重要な前提（どれを使うにしても必須）

**EVM互換のJSON-RPC**が必要。
最低限、以下が揃っていると導入が早い:

* `eth_blockNumber`
* `eth_getBlockByNumber`
* `eth_getTransactionReceipt`
* （イベントを見るなら）`eth_getLogs`

---

## WebSocket無しで進める場合のポイント

* indexerは **ポーリング**で head を追う
* `eth_blockNumber` を定期取得 → 足りない分を順に fetch
* ポーリング間隔は 1〜5秒を目安に調整

---

## おすすめの選び方（Phase2.1向け）

* **最短でExplorerが欲しい** → Blockscout
* **indexerだけOSS、UIは最小自作** → Shovel + Postgres
* **Bridgeイベント中心のAPIが欲しい** → Ponder

---

## 最短の進め方（推奨フロー）

1) Shovel + Postgres で履歴を外出し（WS不要）
2) UIは Next.js で「head / blocks / tx / receipt」だけ作る
3) RPC互換が育ったら Blockscout に差し替え or 併用

これで Phase2.1 の「pruning後でも履歴追える」を最短で満たせる。
