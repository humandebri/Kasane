# Kasane testnet deployment

Kasane testnet固有の設定を置く場所。

公開OSS repo `ic-evm-stack` には入れない値:

- canister id
- RPC URL
- Explorer URL
- indexer cursor起点
- gateway/explorer/indexer運用env
- mainnet/testnet固有のsmoke対象tx

現行情報はroot `README.md` と既存ops docsを正本とする。`ic-evm-stack` のrelease tag固定後、このディレクトリへenv exampleを移す。
