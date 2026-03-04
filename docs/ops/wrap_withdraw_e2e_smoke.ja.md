# wrap withdraw E2E smoke (local)

目的: `pull成功 -> mint失敗 -> withdraw成功` を実ledger経由で確認する。

## 前提

- localネットワーク起動済み
- `evm_canister` と `wrap_canister` を deploy 済み
- ICRC-2 ledger canister があり、テストユーザーが残高を持つ
- テストユーザーが `wrap_canister` に `icrc2_approve` 済み

## 手順

1. `submit_wrap_request` を投げる（mint失敗する条件にする）
   - 例: `evm_nonce` を意図的に不正値にして `submit_ic_tx` を失敗させる
2. `get_wrap_request_result(request_id)` を確認
   - `status = Failed`
   - `pull_ledger_tx_id != null`
   - `mint_tx_id = null`
   - `mint_failed_recoverable = true`
   - `withdrawn = false`
3. request作成者で `withdraw_failed_wrap(request_id)` を呼ぶ
4. `get_wrap_request_result(request_id)` を再確認
   - `withdrawn = true`
   - `withdraw_ledger_tx_id != null`
   - `mint_failed_recoverable = false`
5. 同じ `request_id` で再度 `withdraw_failed_wrap` を呼び、拒否を確認
   - `withdraw.already_withdrawn`

## 期待結果

- mint失敗時に資産が回収可能状態へ遷移する
- 作成者のみ返金可能
- 二重withdraw不可
