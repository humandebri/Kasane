# Resync, Pagination, Missing Data

## TL;DR
- cursorは `block_number/segment/byte_offset` のJSON固定。
- `Err.Pruned` 返却時は cursorを補正して継続する設計。

## 落とし穴
- indexer停止中にprune進行して履歴欠落
- segment上限不一致で復旧不能

## 根拠
- `tools/indexer/README.md`
- `docs/specs/indexer-v1.md`
- `tools/indexer/src/config.ts`
