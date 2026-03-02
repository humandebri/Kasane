# Gateway API Compatibility Baseline

このディレクトリは、Gateway の API 互換ベースラインの正本です。

## 含まれるファイル
- `gateway-api-compat-baseline.did`
  - Gateway が依存する最小 Candid API ベースライン（v1）
- `gateway-api-compat-methods.txt`
  - ベースライン対象メソッド一覧（単一ソース）

## 運用ルール
- 互換破壊を伴う変更時は、同一PRで次を更新する。
  - `gateway-api-compat-baseline.did`
  - `gateway-api-compat-methods.txt`
  - `tools/rpc-gateway/README.md` の互換マトリクス
- CIガード: `scripts/check_gateway_api_compat_baseline.sh`
