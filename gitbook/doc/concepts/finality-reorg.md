# Finality & Reorg

## TL;DR
- 本実装は単一シーケンサ前提で、reorg前提挙動を提供しない。
- submitとexecuteは分離されるため、receipt監視が必要。

## 見え方
- `eth_sendRawTransaction` は投入
- 実行確定は後続blockで反映

## 落とし穴
- submit成功時点で状態確定したとみなす
- Ethereum L1のfork前提ロジックをそのまま持ち込む

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/README.md`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/README.md`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-wrapper/src/lib.rs`
