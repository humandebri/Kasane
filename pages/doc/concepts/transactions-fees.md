# Transactions & Fees

## TL;DR
- 対応tx typeは Legacy/2930/1559、4844/7702は非対応。
- feeは base fee + priority fee モデル（制限付き）。

## 手数料モデル
- `compute_next_base_fee` で次ブロックbase fee更新
- `compute_effective_gas_price` で実効価格を計算

## 落とし穴
- chain id不一致txを送る
- priority/max feeの整合条件を満たさない

## 根拠
- `crates/evm-core/src/tx_decode.rs`
- `crates/evm-core/src/base_fee.rs`
- `crates/evm-core/src/revm_exec.rs`
