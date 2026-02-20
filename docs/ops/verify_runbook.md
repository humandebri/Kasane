# Verify Runbook

## 目的
Explorer Verify の本番運用で事故りやすい3点（solc可用性、メトリクス解釈、鍵ローテ）を手順化する。

## 1. リリース前チェック（solc）
1. verify設定を確認する。
- `EXPLORER_VERIFY_ENABLED=1`
- `EXPLORER_VERIFY_ALLOWED_COMPILER_VERSIONS`（例: `0.8.30,0.8.24`）
2. ワーカー実行ホストで preflight を実行する。
```bash
cd tools/explorer
npm run verify:preflight
```
3. `ok: solc-<version>` が allowlist 全件分出ることを確認する。

## 2. メトリクス運用（再起動時の揺れ対策）
- verifyサンプルは固定窓集計（既定30秒）で保存する。
- ワーカー停止/再起動直後は `success_rate` が不安定になりやすい。
- アラート初期値の推奨:
  - `last_15m success_rate < 80%` を 10分以上継続で通知
  - `current queue depth > 200` を 5分以上継続で通知
- 本番初週は `last_24h` ベースを主指標にする。

## 3. 鍵ローテ（kid）
### 3-1. 事前
- 新鍵を生成し、新 `kid` を決める。
- トークン発行側を「新 `kid` で発行」に切り替える準備をする。

### 3-2. 切替
1. Explorerへ新旧鍵を同時設定:
- `EXPLORER_VERIFY_AUTH_HMAC_KEYS=oldKid:oldSecret,newKid:newSecret`
2. デプロイ後、トークン発行側を新 `kid` に切替。
3. 監視で `unauthorized` の急増がないことを確認。

### 3-3. 旧鍵撤去
1. 最大トークンTTL経過後に旧鍵を削除:
- `EXPLORER_VERIFY_AUTH_HMAC_KEYS=newKid:newSecret`
2. 再デプロイ。

## 4. 監査hash saltローテ
- 現在値: `AUDIT_HASH_SALT_CURRENT`
- 直前値: `AUDIT_HASH_SALT_PREVIOUS`

手順:
1. 新saltを `CURRENT` に設定
2. 直前saltを `PREVIOUS` に設定
3. 次回ローテ時に古い `PREVIOUS` を破棄

## 5. JTIリプレイテーブル運用
- 実装上はワーカー定期処理 + 認証時に期限切れ削除を行う。
- `POST /api/verify/submit` はJTIを消費する（one-time）。
- `GET /api/verify/status` はJTIを消費しない（同一トークンでポーリング可）。
- 障害調査で手動削除する場合:
```sql
DELETE FROM verify_auth_replay
WHERE exp < extract(epoch from now())::bigint;
```

## 6. 重複判定ルール
- verify重複判定は `submitted_by + input_hash` のユーザー単位。
- 同一ユーザーが同一入力を再送した場合は既存requestIdを返す。
- 別ユーザーが同一入力を送った場合は別requestIdを新規発行する。

## 7. 障害時の一次切り分け
1. `compiler_unavailable` が増えた
- `npm run verify:preflight` を再実行
- allowlistと実バイナリ配置を確認
2. `queue_depth` のみ増え続ける
- verify worker 生存確認
- DB接続・ロック競合確認
3. `unauthorized` 急増
- `kid` 切替漏れ、発行側署名鍵、`scope` 不一致を確認
