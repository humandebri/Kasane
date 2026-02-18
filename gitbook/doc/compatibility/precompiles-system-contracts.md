# Precompiles & System Contracts

## TL;DR
- precompile は「存在アドレス一覧」をこのリポジトリから断定できない。
- ただし、precompile失敗が `exec.halt.precompile_error` に分類される事実は確認できる。
- system contract / special address 一覧は本時点で `要確認`。

## できること / できないこと

### できること
- precompile失敗のエラー分類を運用で識別する

### できないこと
- このドキュメント単体で precompile/system contract の正確なアドレス一覧を提示する

## 観測可能な事実
- 実行系エラー分類に `PrecompileError` が存在
- wrapper側で `exec.halt.precompile_error` にマップされる

## 要確認事項
- precompile address一覧
- system contract / reserved address の公式一覧

## 安全な使い方
- precompile依存機能では、`exec.halt.precompile_error` を監視/分類してリトライ判定を分離する
- precompile前提のdAppでは、mainnet投入前に当該opcode/pathをスモークする

## 落とし穴
- 上流EVM一般知識だけでアドレス一覧を断定する
- ingress検証とruntime precompile責務を混同する

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-wrapper/src/lib.rs`（`exec.halt.precompile_error`）
- `/Users/0xhude/Desktop/ICP/Kasane/docs/specs/pr8-signature-boundary.md`
- `/Users/0xhude/Desktop/ICP/Kasane/docs/ops/fixplan2.md`
