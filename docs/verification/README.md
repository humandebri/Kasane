# Verification Architecture

Verus対象コードは `crates/verified-*` に置く。
canister実装はIC runtime、stable memory、Candid、time、cycles、revm呼び出し、hash、codec入出力のadapterに限定する。

## 境界

- `crates/verified-core`: fee、nonce、queue、block、batch、tx index、prune、stable codec、state diffの純粋状態遷移。
  実装関数に `cfg_attr(verus_keep_ghost, verus_spec(...))` を付け、adapterは同じ関数を直接呼ぶ。
- `crates/evm-core`: stable stateの読み書き、revm実行、Candid/API入力、metrics更新。
- `crates/evm-db`: stable memoryのbyte codecとmap key/value型。
- `docs/verification/adapter-contracts.md`: adapter境界ごとのread/write map契約。
- `docs/verification/tcb.md`: Verus対象外依存と未証明ロジックの台帳。

## 追加ルール

- 新規Rust業務ロジックは `crates/verified-*` に追加する。
- Verus対象外に置く場合、`docs/verification/tcb.md` にID、理由、代替検証を登録する。
- adapter層へ分岐を追加する場合、先に純粋関数へ抽出できない理由を確認する。
- fallback/shimで未証明分岐を増やさない。

## 必須検証

```sh
cargo check --workspace
scripts/verify-verus.sh
```

`scripts/verify-verus.sh` は `crates/verified-*/src/lib.rs` を列挙し、`--no-cheating --cfg verus_keep_ghost` で検証する。
`proofs/*.rs` への複製実装は置かない。

CIでは `scripts/check_verification_policy.sh` が `crates/**/*.rs` の変更を検出し、`crates/verified-*` または `docs/verification/tcb.md` の更新を要求する。
`crates/*/src/*.rs` の業務ロジック変更では、PR本文に `verified_core::<function>` または `TCB-<id>` の根拠を書く。
