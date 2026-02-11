## submit_eth_tx の `blob` 仕様

このドキュメントは、`submit_eth_tx` に渡す `blob`（Ethereum signed raw transaction）の作り方と、
`submit -> produce_block -> get_receipt` までの確認手順を示します。  
正本実装は `/Users/0xhude/Desktop/ICP/IC-OP/crates/evm-core/src/tx_decode.rs` と
`/Users/0xhude/Desktop/ICP/IC-OP/crates/ic-evm-wrapper/src/lib.rs` です。

### 1) 入力仕様

`submit_eth_tx(raw_tx: blob)` の `raw_tx` は、署名済み Ethereum transaction の生バイト列です。

- 形式: RLP（Legacy）または Typed Tx（EIP-2930 / EIP-1559）
- 署名: 必須
- 非対応: EIP-4844（type=0x03）, EIP-7702（type=0x04）
- 戻り値: `tx_id`（32 bytes, internal id）
- ガス単位運用: `1 ICP = 10^18` を前提に `gas_price` / `max_fee_per_gas` を解釈する

### 2) 実行例（mainnet）

以下は `icp` での実行例です。  
`ic-evm-core` の `eth_raw_tx` ヘルパーで raw tx を作り、`submit_eth_tx` に渡します。

```bash
CANISTER_ID=4c52m-aiaaa-aaaam-agwwa-cai
IDENTITY=ci-local
CHAIN_ID=4801360

# 送信に使う秘密鍵（32-byte hex, 0xなし）
PRIVKEY="<YOUR_PRIVKEY_HEX>"

# 署名済み raw tx を vec nat8 形式（"1; 2; 3; ..."）で生成
RAW_TX_BYTES=$(cargo run -q -p ic-evm-core --features local-signer-bin --bin eth_raw_tx -- \
  --mode raw \
  --privkey "$PRIVKEY" \
  --to "0000000000000000000000000000000000000001" \
  --value "0" \
  --gas-price "1000000000" \
  --gas-limit "21000" \
  --nonce "0" \
  --chain-id "$CHAIN_ID")

# submit_eth_tx 実行（戻り値は tx_id: blob）
SUBMIT_OUT=$(icp canister call -e ic --identity "$IDENTITY" "$CANISTER_ID" submit_eth_tx "(vec { $RAW_TX_BYTES })")
echo "$SUBMIT_OUT"
```

### 3) tx_id を取り出して receipt を確認

`submit_eth_tx` の戻り値 `Ok : blob` から `tx_id` を抽出し、`get_receipt` に渡します。

```bash
TX_ID_BYTES=$(python - "$SUBMIT_OUT" <<'PY'
import re, sys
text = sys.argv[1]
m = re.search(r'variant\s*\{\s*(?:ok|Ok)\s*=\s*blob\s*\"([^\"]*)\"', text)
if not m:
    raise SystemExit("failed to parse tx_id blob from submit_eth_tx output")
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

# キューからブロック化（manual 実行）
icp canister call -e ic --identity "$IDENTITY" "$CANISTER_ID" produce_block '(1:nat32)'

# receipt 取得
icp canister call -e ic --identity "$IDENTITY" "$CANISTER_ID" get_receipt "(vec { $TX_ID_BYTES })"
```

### 4) 注意点

- `submit_eth_tx` はキュー投入のみで、実行確定は `produce_block`（または auto mine）後です。
- `nonce` は送信元アドレスごとに整合させてください。  
  必要なら `expected_nonce_by_address(blob_address_20bytes)` で事前確認します。
- `eth_tx_hash` を即時に取得したい場合は `rpc_eth_send_raw_transaction` の利用を検討してください。
