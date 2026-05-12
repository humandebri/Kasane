# wrap-canister デプロイ手順（同一サブネット固定）

どこで: mainnet (`ic`)  
何を: `wrap_canister` を既存 `evm_canister` と同一サブネットへ deploy  
なぜ: canister 間連携の遅延/運用リスクを避け、構成を固定するため

---

## 0. 固定パラメータ

- 対象サブネットID（固定）  
  `4ecnw-byqwz-dtgss-ua2mh-pfvs7-c3lct-gtf4e-hnu75-j7eek-iifqm-sqe`

---

## 1. 事前準備

```bash
export ICP_ENV=ic
export ICP_IDENTITY_NAME=<controller_identity>
export WRAP_SUBNET_ID=4ecnw-byqwz-dtgss-ua2mh-pfvs7-c3lct-gtf4e-hnu75-j7eek-iifqm-sqe

# 既存 canister id（環境に合わせて設定）
export EVM_CANISTER_ID=<existing_evm_canister_id>
export KASANE_CANISTER_ID=<existing_kasane_canister_id>
export FEE_LEDGER_CANISTER_ID=<icp_ledger_canister_id>
export NATIVE_LEDGER_CANISTER_ID=<icp_ledger_canister_id>
export ALLOWED_ASSET_CANISTER_ID=<non_native_icrc_ledger_canister_id>

# wrap fee policy（初期値）
export CYCLE_FEE_E8S=1000000
export GAS_PRICE_BUFFER_BPS=12000

# WrapTokenFactory EVM address（20 bytes hex, 0xなし）
export EVM_WRAP_FACTORY_HEX=<40_hex_chars>
export EVM_WRAP_FACTORY_BLOB="$(
  python - <<'PY'
import os
hexv = os.environ["EVM_WRAP_FACTORY_HEX"].strip()
if len(hexv) != 40:
    raise SystemExit("EVM_WRAP_FACTORY_HEX must be 40 hex chars")
raw = bytes.fromhex(hexv)
print(''.join(f'\\{byte:02x}' for byte in raw))
PY
)"
```

注意:

- 既存 factory / token が未稼働なら、監査対応後は新 factory を deploy してこの値へ切り替えてください。
- backward compatibility は持たない前提です。旧 factory は参照しません。
- 新しい `WrapTokenFactory` は `constructor(address minter_)` です。deploy 時は `wrap_canister` 由来の EVM address を constructor に必ず入れてください。
- 現行運用では `KASANE_CANISTER_ID` は `EVM_CANISTER_ID` と同じ principal を入れます。`wrap_canister` は unwrap dispatch caller を `kasane_canister` と照合します。
- `fee_ledger_canister` / `cycle_fee_e8s` / `gas_price_buffer_bps` は既存 mainnet 設定を維持する場合、upgrade 前に `get_fee_policy` で現値を取得してそのまま渡してください。
- `ALLOWED_ASSET_CANISTER_ID` は `NATIVE_LEDGER_CANISTER_ID` 以外にしてください。ICP ledger は native bridge 正本なので factory wrap 対象外です。
- `wrap_factory_address` は Candid 上 `blob` です。`vec { ... }` ではなく、必ず `blob "\xx..."` 形式で渡してください。

前提確認:

```bash
icp canister status "${EVM_CANISTER_ID}" -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}"
```

deploy順序:

- 監査対応版は `evm_canister` upgrade -> `wrap_canister` upgrade -> `wrapper-vite` / frontend deploy の順で反映します。
- `wrapper-vite` は MetaMask unwrap の request id 解決で `evm_canister.get_unwrap_request_ids_by_eth_tx_hash` を呼ぶため、frontend を gateway より先に出すと unwrap 後の追跡に失敗します。

---

## 2. wrap_canister 作成（初回のみ）

`wrap_canister` が未作成の場合のみ実行:

```bash
icp canister create wrap_canister \
  -e "${ICP_ENV}" \
  --identity "${ICP_IDENTITY_NAME}" \
  --subnet "${WRAP_SUBNET_ID}"
```

この `--subnet` 指定で、`wrap_canister` は必ず対象サブネットに作成されます。

---

## 3. wasm build

```bash
cargo build -p wrap-canister --target wasm32-unknown-unknown --release
```

wasm:

`target/wasm32-unknown-unknown/release/wrap_canister.wasm`

---

## 4. install（初回）/upgrade（更新）

重要:

- コード上、`post_upgrade(args: Option<InitArgs>)` は `null` / `opt none` / 引数省略を受け付けず、必ず `opt record {...}` が必要です。
- `wrap_factory_address` は `blob "\xx..."` 形式で渡します。`vec { ... }` は使いません。

### 4-1. 初回 install

```bash
icp canister install wrap_canister \
  -e "${ICP_ENV}" \
  --identity "${ICP_IDENTITY_NAME}" \
  --mode install \
  --wasm target/wasm32-unknown-unknown/release/wrap_canister.wasm \
  --args "(record {
    kasane_canister = principal \"${KASANE_CANISTER_ID}\";
    evm_gateway_canister = principal \"${EVM_CANISTER_ID}\";
    fee_ledger_canister = principal \"${FEE_LEDGER_CANISTER_ID}\";
    native_ledger_canister = principal \"${NATIVE_LEDGER_CANISTER_ID}\";
    wrap_factory_address = blob \"${EVM_WRAP_FACTORY_BLOB}\";
    cycle_fee_e8s = ${CYCLE_FEE_E8S} : nat64;
    gas_price_buffer_bps = ${GAS_PRICE_BUFFER_BPS} : nat32;
    allowed_assets = vec { principal \"${ALLOWED_ASSET_CANISTER_ID}\" };
  })"
```

### 4-2. 更新 upgrade

`upgrade` でも `InitArgs` は必須です。install と同じ Candid を `--args` で渡し、runtime config を明示的に上書きします。

```bash
icp canister install wrap_canister \
  -e "${ICP_ENV}" \
  --identity "${ICP_IDENTITY_NAME}" \
  --mode upgrade \
  --wasm target/wasm32-unknown-unknown/release/wrap_canister.wasm \
  --args "(opt record {
    kasane_canister = principal \"${KASANE_CANISTER_ID}\";
    evm_gateway_canister = principal \"${EVM_CANISTER_ID}\";
    fee_ledger_canister = principal \"${FEE_LEDGER_CANISTER_ID}\";
    native_ledger_canister = principal \"${NATIVE_LEDGER_CANISTER_ID}\";
    wrap_factory_address = blob \"${EVM_WRAP_FACTORY_BLOB}\";
    cycle_fee_e8s = ${CYCLE_FEE_E8S} : nat64;
    gas_price_buffer_bps = ${GAS_PRICE_BUFFER_BPS} : nat32;
    allowed_assets = vec { principal \"${ALLOWED_ASSET_CANISTER_ID}\" };
  })"
```

---

## 5. 反映確認

```bash
icp canister status wrap_canister -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}"
```

必要に応じて fee policy の現値確認:

```bash
icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query wrap_canister get_fee_policy '()'
```

必要に応じて did インターフェース確認:

```bash
dfx canister call --query wrap_canister export_did '()' --network "${ICP_ENV}"
```

---

## 6. 運用上の注意

- `submit_wrap_request` は wallet caller 本人で実行され、`from_owner` は canister 側で `msg_caller` 固定です（引数で渡しません）。
- `submit_wrap_request` は直前の `quote_wrap_request` から得た `charged_fee_e8s` / `charged_gas_price_wei` / `fee_ledger_canister` を上限として渡します。超過・ledger変更時は送金前に拒否されます。
- Wrap手数料（`cycles + gas`）は `fee_ledger_canister` から `icrc2_transfer_from` で前払い徴収されます。
- wrap mint 時の decimals は対象 ledger の `icrc1_metadata` を一次情報として取得します。metadata が壊れている ledger は wrap できません。
- wrap 対象 asset は on-chain allowlist に載っている principal だけです。初回 install / upgrade の `allowed_assets` で明示してください。
- `native_ledger_canister` は Kasane native ICP bridge の正本 ledger です。`allowed_assets` に同じ principal を入れると `asset.native_ledger_not_wrappable` で拒否されます。
- native ICP withdraw は `quote_native_withdrawal` で `ledger_fee_e8s` と `receive_amount_e8s` を取得してからEVM txを送信してください。
- native withdraw precompile は低水準APIです。fee以下の `msg.value` を直接送るとv1ではrefund対象外です。
- `set_fee_policy` は controller のみ実行可能です。例:

```bash
icp canister call -e "${ICP_ENV}" wrap_canister set_fee_policy '(record {
  fee_ledger_canister = principal "'"${FEE_LEDGER_CANISTER_ID}"'";
  cycle_fee_e8s = 1000000 : nat64;
  gas_price_buffer_bps = 12000 : nat32;
})'
```

- allowlist を後から全置換する場合も controller のみ実行可能です。例:

```bash
icp canister call -e "${ICP_ENV}" wrap_canister set_allowed_assets '(vec {
  principal "'"${ALLOWED_ASSET_CANISTER_ID}"'";
})'
```

- もし既存 `wrap_canister` が別サブネット上にある場合、同一 canister id のまま移動はできません。  
  新規 canister を対象サブネットで作成し、参照先（`wrap_canister_id`）を切り替えてください。
