# Wrap Factory Contracts

このディレクトリには、ICRC-2 canister id をキーに EVM 側 wrapped token を `CREATE2` で決定的生成する最小実装を置いています。

- `WrapTokenFactory.sol`
  - `mintForAsset(bytes canisterId, address to, uint256 amount)`
  - `salt = keccak256("kasane.wrap.v1", chain_id, canister_id_bytes)`
  - 未デプロイ時のみ `CREATE2` で token を作成し、その後 mint
- `WrappedAssetToken.sol`
  - factory のみ `mint` 可能な最小 ERC-20

## 運用メモ

- `WrapTokenFactory` の `minter` には、`wrap-canister` principal から導出した EVM アドレスを設定してください。
- `wrap-canister` 側は factory へ `mintForAsset` を呼びます。
- 同一 `chain_id` + 同一 `canister_id` では同じ token address が再現されます。

## 開発メモ

- コンパイル: `cd contracts && forge build`
- テスト: `cd contracts && forge test -vv`
- CI でも `forge build` / `forge test` を実行し、`predictTokenAddress` と `mintForAsset` の互換を検証します。
