#!/usr/bin/env bash
# where: local CI/smoke shared helper
# what: parse `icp canister call` textual Candid result safely
# why: avoid duplicated fragile regex checks and centralize fallback policy

set -euo pipefail

_CANDID_RESULT_DIDC_WARNED=0

_candid_result_warn_missing_didc_once() {
  if [[ "${_CANDID_RESULT_DIDC_WARNED}" -eq 0 ]]; then
    echo "[candid-result] WARN: didc not found; using parser fallback" >&2
    _CANDID_RESULT_DIDC_WARNED=1
  fi
}

_candid_encode_or_fail() {
  local text="${1-}"
  if command -v didc >/dev/null 2>&1; then
    didc encode "${text}"
    return 0
  fi
  _candid_result_warn_missing_didc_once
  return 1
}

_candid_extract_value_text() {
  local text="${1-}"
  CANDID_RESULT_TEXT="${text}" python - <<'PY'
import os
import sys

text = os.environ.get("CANDID_RESULT_TEXT", "")
start = text.find("(")
if start < 0:
    sys.exit(1)

depth = 0
in_string = False
escaped = False
for i in range(start, len(text)):
    ch = text[i]
    if in_string:
        if escaped:
            escaped = False
            continue
        if ch == "\\":
            escaped = True
            continue
        if ch == '"':
            in_string = False
        continue
    if ch == '"':
        in_string = True
        continue
    if ch == "(":
        depth += 1
        continue
    if ch == ")":
        depth -= 1
        if depth == 0:
            print(text[start : i + 1])
            sys.exit(0)

sys.exit(1)
PY
}

_candid_ok_labels_csv() {
  if command -v didc >/dev/null 2>&1; then
    local ok_hash
    local ok_lower_hash
    ok_hash="$(didc hash Ok)"
    ok_lower_hash="$(didc hash ok)"
    echo "Ok,ok,${ok_hash},${ok_lower_hash},17_724"
    return 0
  fi
  echo "Ok,ok,17724,17_724"
}

candid_is_ok() {
  local text="${1-}"
  if [[ -z "${text}" ]]; then
    return 1
  fi
  local value_text
  value_text="$(_candid_extract_value_text "${text}" 2>/dev/null || true)"
  if [[ -z "${value_text}" ]]; then
    value_text="${text}"
  fi
  local ok_labels
  ok_labels="$(_candid_ok_labels_csv)"

  if command -v didc >/dev/null 2>&1; then
    local encoded
    local decoded
    encoded="$(_candid_encode_or_fail "${value_text}")" || return 1
    decoded="$(didc decode "${encoded}")" || return 1
    CANDID_RESULT_TEXT="${decoded}" CANDID_OK_LABELS="${ok_labels}" python - <<'PY'
import os
import re
import sys

text = os.environ.get("CANDID_RESULT_TEXT", "")
labels = set(filter(None, os.environ.get("CANDID_OK_LABELS", "").split(",")))
# Accept both:
# - variant { Ok = ... }
# - variant { 17_724 }
m = re.search(r"variant\s*\{\s*([^=\s\}]+)\s*(?:=|\})", text, re.S)
if not m:
    sys.exit(1)
label = m.group(1).strip()
sys.exit(0 if label in labels else 1)
PY
    return $?
  fi

  _candid_result_warn_missing_didc_once

  CANDID_RESULT_TEXT="${value_text}" CANDID_OK_LABELS="${ok_labels}" python - <<'PY'
import os
import re
import sys

text = os.environ.get("CANDID_RESULT_TEXT", "")
labels = set(filter(None, os.environ.get("CANDID_OK_LABELS", "").split(",")))
# Accept both:
# - variant { Ok = ... }
# - variant { 17_724 }
m = re.search(r"variant\s*\{\s*([^=\s\}]+)\s*(?:=|\})", text, re.S)
if not m:
    sys.exit(1)
label = m.group(1).strip()
sys.exit(0 if label in labels else 1)
PY
}

candid_extract_ok_blob_bytes() {
  local text="${1-}"
  if [[ -z "${text}" ]]; then
    return 1
  fi
  local value_text
  value_text="$(_candid_extract_value_text "${text}" 2>/dev/null || true)"
  if [[ -z "${value_text}" ]]; then
    value_text="${text}"
  fi
  local ok_labels
  ok_labels="$(_candid_ok_labels_csv)"

  if command -v didc >/dev/null 2>&1; then
    local encoded
    local decoded
    encoded="$(_candid_encode_or_fail "${value_text}")" || return 1
    decoded="$(didc decode "${encoded}")" || return 1
    CANDID_RESULT_TEXT="${decoded}" CANDID_OK_LABELS="${ok_labels}" python - <<'PY'
import os
import re
import sys

text = os.environ.get("CANDID_RESULT_TEXT", "")
labels = set(filter(None, os.environ.get("CANDID_OK_LABELS", "").split(",")))
variant = re.search(
    r'variant\s*\{\s*([^=\s]+)\s*=\s*blob\s*"((?:[^"\\]|\\.)*)"',
    text,
    re.S,
)
if not variant:
    sys.exit(1)
label = variant.group(1).strip()
if label not in labels:
    sys.exit(1)
s = variant.group(2)
out = bytearray()
i = 0
while i < len(s):
    if s[i] == '\\':
        if i + 2 < len(s) and all(c in "0123456789abcdefABCDEF" for c in s[i + 1 : i + 3]):
            out.append(int(s[i + 1 : i + 3], 16))
            i += 3
            continue
        if i + 1 < len(s):
            out.append(ord(s[i + 1]))
            i += 2
            continue
    out.append(ord(s[i]))
    i += 1
print("; ".join(str(b) for b in out))
PY
    return $?
  fi

  _candid_result_warn_missing_didc_once

  CANDID_RESULT_TEXT="${value_text}" CANDID_OK_LABELS="${ok_labels}" python - <<'PY'
import os
import re
import sys

text = os.environ.get("CANDID_RESULT_TEXT", "")
labels = set(filter(None, os.environ.get("CANDID_OK_LABELS", "").split(",")))

variant = re.search(
    r'variant\s*\{\s*([^=\s]+)\s*=\s*blob\s*"((?:[^"\\]|\\.)*)"',
    text,
    re.S,
)
if not variant:
    sys.exit(1)

label = variant.group(1).strip()
if label not in labels:
    sys.exit(1)

s = variant.group(2)
out = bytearray()
i = 0
while i < len(s):
    if s[i] == '\\':
        if i + 2 < len(s) and all(c in "0123456789abcdefABCDEF" for c in s[i + 1 : i + 3]):
            out.append(int(s[i + 1 : i + 3], 16))
            i += 3
            continue
        if i + 1 < len(s):
            out.append(ord(s[i + 1]))
            i += 2
            continue
    out.append(ord(s[i]))
    i += 1

print("; ".join(str(b) for b in out))
PY
}
