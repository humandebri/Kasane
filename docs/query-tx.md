# IC価格Queryを同じEVM txで使う

`0x00000000000000000000000000000000ffff0003`は、許可された通常queryのraw Candid返答をEVM txへ返す。1 txにつき外部呼び出しは1回、bounded waitは2秒固定。composite queryはtxでは利用できない。ICの通常canister間callで合意を通して実行するため、controllerは登録メソッドが通常queryであることを確認する必要がある。対象がupgradeでupdateメソッドに変わると外部副作用を起こし得る。対象canisterとそのcontroller・upgrade管理は信頼境界に含む。[IC仕様](https://docs.internetcomputer.org/references/ic-interface-spec/canister-interface/)

## 登録と監視

upgrade時のtx用許可リストは空。既存のeth_call用許可リストとは別に登録する。

```sh
dfx canister call <KASANE> add_tx_query_precompile_allowed_method '(record { target = principal "<ORACLE>"; method = "price" })'
dfx canister call --query <KASANE> get_tx_query_precompile_allowlist '()'
dfx canister call --query <KASANE> get_pending_query_tx '()'
dfx canister call <KASANE> remove_tx_query_precompile_allowed_method '(record { target = principal "<ORACLE>"; method = "price" })'
```

追加・削除はcontroller限定。待機中は設定変更とpruningがBusyになる。直接credit APIは再試行可能な`ic_query.tx_busy`を返し、拒否したcreditを自動適用しない。内部native depositは引き落とし後のRunning/Pulledとqueueに保存し、Query tx終了後にcreditする。

## ABIとコントラクト

compact要求は`version:u8=1 | kind:u8=0 | principal_len:u8 | principal | method_len:u8 | ASCII method | arg_len:u32 big-endian | raw Candid args`。返答は再エンコードしない。例は[PriceTrade.sol](../examples/query-tx/PriceTrade.sol)。この例は単一nat64返答専用で、価格・msg.value・期限を検査する。外部canisterの更新は停止しないため、実運用のoracleには観測時刻を含め、コントラクトで鮮度とスリッページを検査する。

`eth_estimateGas`は新しいcomposite query API `rpc_eth_estimate_gas_object_at_with_query_precompile`を使う。外部返答を1回取得し二分探索で再利用する。探索中にtarget・method・引数が変わる契約は要求不一致エラーとなる。推定は非replicatedの読み取りであり、実txの読み取りと価格が一致する保証はない。IC composite queryは同一subnetの呼び出しに限定されるため、別subnetのoracleではこの推定APIは失敗する。replicated tx自体は別subnetにも対応する。従来のeth_call用APIと同期gas推定APIも維持する。

## 実行・復旧

Query検出時の差分、nonce、ログ、料金は捨てる。先行通常txは先にsealし、その直後にQuery txを予約する。親head・block context・EVM epoch・設定のfingerprint・update intent容量を固定し、応答を注入して再実行する。Query txは単独ブロック。後続txは受付を継続するが追い越さない。予約txはnonce差し替えとevictionから除外する。

stable sessionのattempt IDとCalling状態が一致するcallbackだけを受理する。callbackは返答保存まで、確定は別timer。reject・過大応答・期限切れはprecompile失敗になり、契約が捕捉すれば成功処理を続行できる。料金とreceiptは確定実行1回分のみ。2秒後のtimerは処理可能な時点で動き、実時間2秒以内の解除は保証しない。停止中はsealしない。再開時に期限を再検査する。

upgradeでは呼び出しを再送せずinterruptedを注入する。固定状態が変わっていればdrop code 11で終了し、古い成功応答は適用しない。stable metricsの既存固定領域を保つため、そのdropの集計はexecution dropへ加算する。保存領域破損は運用停止にする。

## ローカル検証と導入

```sh
cargo check --workspace
cargo test -p ic-evm-core --test query_tx
cargo test -p ic-evm-gateway --lib
scripts/run_query_tx_e2e.sh
CI_LOCAL_MODE=github scripts/ci-local.sh
scripts/predeploy_smoke.sh
```

E2Eは価格canisterとEVM契約をdeployし、通常読み取りの41に対してreplicated txが42を読み、storageと送金が同じtxで確定することを確認する。PocketIC 12の実行環境に合うバイナリを`POCKET_IC_BIN`で指定できる。

本番導入は別作業。許可リスト空でupgradeし、対象の通常query宣言とupgrade管理を確認して登録する。同一subnet・別subnetで2秒以内の成功率と待機時間を測る。rollback前はtx受付を停止し、pending Query txと保留native depositを解消する。

Hostのfailpoint試験は保存済み返答を確定前に消さないことを検査する。IC messageのtrapによる全stable writeのrollbackはHostメモリでは再現されないため、その全体原子性はIC実行環境の保証に依存する。
