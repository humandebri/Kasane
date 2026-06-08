# `rpc_eth_send_raw_transaction` Blob Payload

This guide shows how to build the `blob` argument for `rpc_eth_send_raw_transaction` and how to follow the transaction from submit to receipt.

Canonical implementation references:

- `crates/evm-core/src/tx_decode.rs`
- `crates/ic-evm-gateway/src/lib.rs`

## Input Contract

`rpc_eth_send_raw_transaction(raw_tx: blob)` accepts raw bytes for a signed Ethereum transaction.

- Format: Legacy RLP or typed transaction (`EIP-2930`, `EIP-1559`).
- Signature: required.
- Unsupported: `EIP-4844` (`type=0x03`) and `EIP-7702` (`type=0x04`).
- Return value: internal `tx_id` (`32` bytes).
- Gas unit policy: `gas_price` and `max_fee_per_gas` are interpreted with `1 ICP = 10^18` base units.

## Submit Example

```bash
CANISTER_ID=4c52m-aiaaa-aaaam-agwwa-cai
IDENTITY=ci-local
CHAIN_ID=4801360
PRIVKEY="<YOUR_PRIVKEY_HEX>"

RAW_TX_BYTES=$(cargo run -q -p ic-evm-core --features local-signer-bin --bin eth_raw_tx -- \
  --mode raw \
  --privkey "$PRIVKEY" \
  --to "0000000000000000000000000000000000000001" \
  --value "0" \
  --gas-price "1000000000" \
  --gas-limit "21000" \
  --nonce "0" \
  --chain-id "$CHAIN_ID")

SUBMIT_OUT=$(icp canister call -e ic --identity "$IDENTITY" "$CANISTER_ID" rpc_eth_send_raw_transaction "(vec { $RAW_TX_BYTES })")
echo "$SUBMIT_OUT"
```

## Extract `tx_id` and Read Receipt

```bash
TX_ID_BYTES=$(python - "$SUBMIT_OUT" <<'PY'
import re, sys
text = sys.argv[1]
m = re.search(r'variant\s*\{\s*(?:ok|Ok)\s*=\s*blob\s*\"([^\"]*)\"', text)
if not m:
    raise SystemExit("failed to parse tx_id blob from rpc_eth_send_raw_transaction output")
s = m.group(1)
out = bytearray()
i = 0
while i < len(s):
    if s[i] == '\\':
        if i + 2 < len(s) and all(c in "0123456789abcdefABCDEF" for c in s[i+1:i+3]):
            out.append(int(s[i+1:i+3], 16))
            i += 3
            continue
        if i + 1 < len(s):
            out.append(ord(s[i+1]))
            i += 2
            continue
    out.append(ord(s[i]))
    i += 1
print('; '.join(str(b) for b in out))
PY
)

dfx canister call --query "$CANISTER_ID" rpc_eth_block_number '( )' --network=ic
dfx canister call --query "$CANISTER_ID" get_receipt "(vec { $TX_ID_BYTES })" --network=ic
```

## Notes

- `rpc_eth_send_raw_transaction` only enqueues the transaction. Final execution happens after block production.
- Keep `nonce` consistent for the sender address. Use `expected_nonce_by_address(blob_address_20bytes)` before submit when needed.
- `eth_tx_hash` is not returned directly by `rpc_eth_send_raw_transaction`; use the hash-oriented RPC methods when Ethereum-compatible lookup is needed.
