# Wrap Factory Contracts

This directory contains the minimal EVM-side implementation for deterministic wrapped-token deployment with `CREATE2`, keyed by ICRC-2 canister id.

- `WrapTokenFactory.sol`
  - `mintForAsset(bytes canisterId, uint8 decimals, address to, uint256 amount)`
  - `predictTokenAddress(bytes canisterId, uint8 decimals)`
  - `salt = keccak256("kasane.wrap.v1", chain_id, canister_id_bytes)`
  - canonical asset id is `Principal::as_slice()` raw bytes
  - creates the token with `CREATE2` only when it does not exist, then mints
- `WrappedAssetToken.sol`
  - minimal ERC-20 token
  - only the factory can mint
  - burns must go through the factory

## Operations

- Set `WrapTokenFactory.minter` to the EVM address derived from the `evm_canister` principal.
- Deployment creation data is `bytecode || abi.encode(constructor(address minter_))`; do not omit the constructor argument.
- The canister reads ledger metadata `icrc1:decimals` and calls `mintForAsset`.
- The same `chain_id`, `canister_id`, and `decimals` reproduce the same token address.
- `WrappedAssetToken.burn` and `burnFrom` are disabled; unwrap burns go through `WrapTokenFactory.burnFromAsset`.
- Treat `minter` as a trusted supply boundary with the same severity as the EVM canister.

## Development

```bash
cd tools/wrapper-vite/contracts
forge build
forge test -vv
```

CI should run `forge build` and `forge test`, covering `predictTokenAddress(bytes,uint8)`, `mintForAsset`, and `burnFromAsset` compatibility.
