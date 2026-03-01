# Precompiles & System Contracts

## TL;DR
- precompile 失敗は `exec.halt.precompile_error` として分類される。
- precompile 依存機能は、事前スモークと実行時監視を前提に運用する。

## 運用方針
- precompile失敗の分類は `exec.halt.precompile_error` を一次判定に使う。
- precompile依存機能は mainnet 投入前に対象パスをスモークして動作を固定化する。

## 観測可能な事実
- 実行系エラー分類に `PrecompileError` が存在
- wrapper側で `exec.halt.precompile_error` にマップされる

## 安全な使い方
- precompile依存機能では、`exec.halt.precompile_error` を監視/分類してリトライ判定を分離する
- precompile前提のdAppでは、mainnet投入前に当該opcode/pathをスモークする

## 落とし穴
- ingress検証とruntime precompile責務を混同する

## 根拠
- `crates/ic-evm-wrapper/src/lib.rs`（`exec.halt.precompile_error`）
- `docs/specs/pr8-signature-boundary.md`
- `docs/ops/fixplan2.md`
