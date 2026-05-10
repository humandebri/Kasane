# Wrap Factory Contracts

このディレクトリには、ICRC-2 canister id をキーに EVM 側 wrapped token を `CREATE2` で決定的生成する最小実装を置いています。

- `WrapTokenFactory.sol`
  - `mintForAsset(bytes canisterId, uint8 decimals, address to, uint256 amount)`
  - `predictTokenAddress(bytes canisterId, uint8 decimals)`
  - `salt = keccak256("kasane.wrap.v1", chain_id, canister_id_bytes)`
  - canonical asset id は `Principal::as_slice()` の raw bytes
  - 未デプロイ時のみ `CREATE2` で token を作成し、その後 mint
- `WrappedAssetToken.sol`
  - factory のみ `mint` 可能な最小 ERC-20
  - burn は factory 経由のみ

## 運用メモ

- `WrapTokenFactory` の `minter` には、`evm_canister` principal から導出した EVM アドレスを設定してください。
- deploy 時の creation data は `bytecode || abi.encode(constructor(address minter_))` です。constructor 引数を省くと `minter` が壊れるので、そのまま deploy しないでください。
- `evm_canister` 側は ledger metadata の `icrc1:decimals` を取得して factory へ `mintForAsset` を呼びます。
- 同一 `chain_id` + 同一 `canister_id` + 同一 `decimals` では同じ token address が再現されます。
- `WrappedAssetToken.burn` / `burnFrom` は無効化されており、unwrap burn は必ず `WrapTokenFactory.burnFromAsset` を通ります。
- `minter` は wrapped supply を直接増やせる信頼境界です。運用では `evm_canister` と同じ厳しさで扱ってください。

## 開発メモ

- コンパイル: `cd tools/wrapper-vite/contracts && forge build`
- テスト: `cd tools/wrapper-vite/contracts && forge test -vv`
- CI でも `forge build` / `forge test` を実行し、`predictTokenAddress(bytes,uint8)` と `mintForAsset` / `burnFromAsset` の互換を検証します。
