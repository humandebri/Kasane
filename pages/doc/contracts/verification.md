# Verification

## TL;DR
- Verify 機能は Explorer 側で提供される。
- 事前チェック・鍵ローテーション・監視運用は `docs/ops/verify_runbook.md` を正本として運用する。

## 運用手順（概要）
1. `EXPLORER_VERIFY_ENABLED=1` と許可コンパイラの環境変数を設定する。
2. `tools/explorer` で `npm run verify:preflight` を実行し、許可バージョンの `solc` 可用性を確認する。
3. `POST /api/verify/submit` で検証ジョブを投入し、`GET /api/verify/status` で状態を確認する。

## 根拠
- `tools/explorer/README.md`
- `docs/ops/verify_runbook.md`
