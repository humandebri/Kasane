# Signing, Nonce, Retries

## TL;DR
- nonceは送信前に必ず取得する。
- chain idは `4801360` を一致させる。
- submit成功後は receipt poll で結果判定する。

## 推奨手順
1. sender addressを確定
2. `eth_getTransactionCount` または `expected_nonce_by_address`
3. fee設定（base fee + priorityを考慮）
4. `eth_sendRawTransaction`
5. `eth_getTransactionReceipt` をpoll

## 落とし穴
- nonce固定値ハードコード
- receipt timeout時に再送して nonce競合

## 根拠
- `README.md`
- `tools/rpc-gateway/README.md`
- `crates/ic-evm-wrapper/src/lib.rs`
