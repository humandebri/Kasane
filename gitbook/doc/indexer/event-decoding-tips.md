# Event Decoding Tips

## TL;DR
- ERC-20 Transfer抽出は topic/data長チェックを厳格に行う。
- 不正ログは行単位スキップし、全体停止を避ける設計。

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/tools/indexer/README.md`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/indexer/src/decode_receipt.ts`
