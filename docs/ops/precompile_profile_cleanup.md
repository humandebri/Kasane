# Precompile Profile Cleanup Memo

## 目的
- precompile の instruction/cycle 比課金を決めるための計測導線は残す
- ただし、本番 canister に計測専用 API を常設しない
- 運用導線は script / PocketIC 側に寄せる

## 現状
- canister 内に profile 集計本体がある
  - precompile 実行時の instruction / extra_gas を集計
- canister state に precompile ratio は保持しない
  - 既定 build は fixed ratio `1/100` をコードで持つ
- canister API がある
  - `get_precompile_profile`
  - `clear_precompile_profile`
  - `profile_precompile_call`
- `profile_precompile_call` は PocketIC / local 計測用の update API
  - `query` では profile が永続化されないため追加した

## 結論
- script に寄せるのは賛成
- ただし、計測値を canister 内で集計する都合上、計測ビルドでは最小限の canister 側コードは必要
- 本番では `profile_precompile_call` は消すか feature gate で無効化するのがよい

## 推奨構成
1. 常設してよいもの
- precompile 実行時の profile 集計本体
- 固定 extra gas ratio（既定 build は `1/100`）

2. 条件付きにしたいもの
- `get_precompile_profile`
- `clear_precompile_profile`
- `profile_precompile_call`

3. 運用の主導線
- `scripts/run_precompile_profile_e2e.sh`
- `scripts/measure_precompile_ratio.sh`

## 実装方針
### A. 一番おすすめ
- cargo feature を追加する
  - 例: `precompile-profile-admin`
- この feature が有効なときだけ以下を公開する
  - `get_precompile_profile`
  - `clear_precompile_profile`
  - `profile_precompile_call`
- PocketIC / local build では feature を ON
- mainnet build では feature を OFF

### B. さらに締める場合
- profile 集計本体も feature gate する
  - 例: `precompile-profile`
- mainnet で runtime overhead を完全に消したいならこの形
- ただし、本番で再計測したくなったときは再デプロイ前提になる

### C. 非推奨
- 本番 canister に `profile_precompile_call` を残し続ける
- controller-only でも、計測専用 update 面を増やす価値は小さい

## 判断メモ
- `profile_precompile_call` は本番に不要
- `get_precompile_profile` / `clear_precompile_profile` も本番不要なら一緒に閉じる
- 課金ロジック自体は本番で必要なので残す
- profile と ratio 判断は wall-clock ではなく instruction counter を正とする
- `scripts/measure_precompile_ratio.sh` は計測開始前に `clear_precompile_profile` の成功を確認し、権限不足などで失敗した場合は古い profile を混ぜずに停止する
- `get_precompile_profile` は計測 build でも controller query 前提とし、匿名 caller には公開しない
- ratio は runtime API で切り替えず、PocketIC/local で再計測して再デプロイで変える

## 次回やること
1. `profile_precompile_call` を feature gate に移す
2. DID 生成を feature ごとに確認する
3. `scripts/run_precompile_profile_e2e.sh` を feature 有効 build 前提に合わせる
4. mainnet build で計測 API が露出していないことを CI で検証する
