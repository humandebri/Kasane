# Indexer Worker (SQLite-first)

目的: canister の export API を pull して SQLite に最小インデックスを作る。

## 使い方

```bash
cd tools/indexer
npm install
npm run dev
```

## 環境変数

- `INDEXER_CANISTER_ID` (必須)
- `INDEXER_IC_HOST` (任意, 既定: http://127.0.0.1:4943)
- `INDEXER_DB_PATH` (任意, 既定: ./indexer.sqlite)
- `INDEXER_MAX_BYTES` (任意, 既定: 1200000)
- `INDEXER_BACKOFF_INITIAL_MS` (任意, 既定: 200)
- `INDEXER_BACKOFF_MAX_MS` (任意, 既定: 5000)
- `INDEXER_FETCH_ROOT_KEY` (任意, 1/true で有効。local向け)

## Cursor JSON（固定）

```
{
  "v": 1,
  "block_number": "u64",
  "segment": 0,
  "byte_offset": 0
}
```

* block_number は **10進ASCII、先頭0なし**（"0"は許可）
* segment は **0/1/2**
* byte_offset は **0..=u32**
