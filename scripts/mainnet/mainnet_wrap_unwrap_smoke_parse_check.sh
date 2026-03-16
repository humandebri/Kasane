#!/usr/bin/env bash
# where: mainnet wrap/unwrap smoke parser check
# what: wrap canister の代表的な Candid 応答サンプルを解析して抽出値を検証する
# why: mainnet smoke の parser 回帰を本番実行前に軽量に検知するため
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

python - <<'PY'
import json
import re


def decode_blob(raw):
    if raw is None:
        return None
    out = bytearray()
    i = 0
    while i < len(raw):
        if raw[i] == "\\" and i + 2 < len(raw) and all(c in "0123456789abcdefABCDEF" for c in raw[i + 1:i + 3]):
            out.append(int(raw[i + 1:i + 3], 16))
            i += 3
        elif raw[i] == "\\" and i + 1 < len(raw):
            out.append(ord(raw[i + 1]))
            i += 2
        else:
            out.append(ord(raw[i]))
            i += 1
    return out.hex()


def parse_get_request(text):
    status = re.search(r"status\s*=\s*variant\s*\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*\}", text)
    if not status:
        raise SystemExit("status field was not found")

    def capture_opt_blob(label):
        match = re.search(label + r'\s*=\s*opt\s+blob\s+"((?:[^"\\]|\\.)*)"', text)
        return decode_blob(match.group(1)) if match else None

    def capture_opt_nat(label):
        match = re.search(label + r"\s*=\s*opt\s+\(([0-9_]+)\s*:\s*(?:nat|nat64)\)", text)
        return int(match.group(1).replace("_", "")) if match else None

    return {
        "status": status.group(1),
        "mint_tx_id_hex": capture_opt_blob("mint_tx_id"),
        "ledger_tx_id_hex": capture_opt_blob("ledger_tx_id"),
        "charged_fee_e8s": capture_opt_nat("charged_fee_e8s"),
    }


def parse_get_unwrap_requirements(text):
    wrapped = re.search(r'wrapped_token_address\s*=\s*opt\s+blob\s+"((?:[^"\\]|\\.)*)"', text)
    return {
        "wrapped_token_address_hex": decode_blob(wrapped.group(1)) if wrapped else None,
    }


blob_deadbeef = "\\" + "de" + "\\" + "ad" + "\\" + "be" + "\\" + "ef"
blob_aabbccdd = "\\" + "aa" + "\\" + "bb" + "\\" + "cc" + "\\" + "dd"
blob_11223344 = "\\" + "11" + "\\" + "22" + "\\" + "33" + "\\" + "44"

wrap_result = f"""(opt record {{
  status = variant {{ Succeeded }};
  mint_tx_id = opt blob "{blob_deadbeef}";
  ledger_tx_id = null;
  charged_fee_e8s = opt (1_234_567 : nat);
}})"""
wrap_expected = {
    "status": "Succeeded",
    "mint_tx_id_hex": "deadbeef",
    "ledger_tx_id_hex": None,
    "charged_fee_e8s": 1234567,
}
wrap_actual = parse_get_request(wrap_result)
if wrap_actual != wrap_expected:
    raise SystemExit(f"wrap parser mismatch: {wrap_actual}")

unwrap_requirements = f"""(variant {{
  Ok = record {{
    wrapped_token_address = opt blob "{blob_aabbccdd}";
  }}
}})"""
unwrap_requirements_expected = {
    "wrapped_token_address_hex": "aabbccdd",
}
unwrap_requirements_actual = parse_get_unwrap_requirements(unwrap_requirements)
if unwrap_requirements_actual != unwrap_requirements_expected:
    raise SystemExit(
        f"unwrap requirements parser mismatch: {unwrap_requirements_actual}"
    )

unwrap_result = f"""(opt record {{
  status = variant {{ Succeeded }};
  mint_tx_id = null;
  ledger_tx_id = opt blob "{blob_11223344}";
  charged_fee_e8s = opt (99 : nat64);
}})"""
unwrap_expected = {
    "status": "Succeeded",
    "mint_tx_id_hex": None,
    "ledger_tx_id_hex": "11223344",
    "charged_fee_e8s": 99,
}
unwrap_actual = parse_get_request(unwrap_result)
if unwrap_actual != unwrap_expected:
    raise SystemExit(f"unwrap parser mismatch: {unwrap_actual}")

print(json.dumps({"ok": True}))
PY
