#!/usr/bin/env bash
# Reproduce the fixed official ledger artifact; byte equality is not a compiler proof.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
source_dir="$PWD/.local/proof-tools/ic-ledger-source"
cache_dir="$PWD/.local/proof-tools/ledger-repro-cache"
container_name="${LEDGER_REPRO_CONTAINER_NAME:-kasane-ledger-repro-cf41372}"
if container inspect "$container_name" >/dev/null 2>&1; then
    echo "Container $container_name already exists; inspect its state or choose a fresh LEDGER_REPRO_CONTAINER_NAME." >&2
    exit 2
fi
if [[ ! -d "$source_dir" ]]; then
    git clone --depth 1 --branch ledger-suite-icrc-2026-03-09 --single-branch https://github.com/dfinity/ic.git "$source_dir"
fi
python3 scripts/audit_icrc_ledger_artifact.py
image_reference="$(python3 - <<'PY'
from pathlib import Path
import json,hashlib
p=json.load(open('proofs/external/ledger-reproduction-profile.json'))
root=Path('.local/proof-tools/ic-ledger-source')
for mapping in [p['build_input_sha256'],p['image_tag_input_sha256']]:
 for f,h in mapping.items():assert hashlib.sha256((root/f).read_bytes()).hexdigest()==h,f
print(p['image_reference'])
PY
)"
container image pull --platform linux/amd64 "$image_reference"
mkdir -p "$cache_dir"
cat > "$cache_dir/build-ledger.sh" <<'BUILD'
#!/usr/bin/env bash
set -euo pipefail
cd /ic
git config --global --add safe.directory /ic
test "$(git rev-parse HEAD)" = cf41372e3d4dc1accfe2c09a7969f8bddc729dc1
test -z "$(git status --porcelain)"
rustc --version
bazel --output_user_root=/cache/bazel build --config=local --config=stamped --jobs=4 --repository_cache=/cache/repository //rs/ledger_suite/icrc1/ledger:ledger_canister.wasm.gz
cp bazel-bin/rs/ledger_suite/icrc1/ledger/ledger_canister.wasm.gz /cache/ic-icrc1-ledger.rebuilt.wasm.gz
gzip -dc /cache/ic-icrc1-ledger.rebuilt.wasm.gz > /cache/ic-icrc1-ledger.rebuilt.wasm
sha256sum /cache/ic-icrc1-ledger.rebuilt.wasm
BUILD
container run --name "$container_name" --platform linux/amd64 --rosetta --cpus 4 --memory 8g --user 0:0 \
    --env RUSTUP_HOME=/home/ubuntu/.rustup --env CARGO_HOME=/home/ubuntu/.cargo \
    --mount "source=$source_dir,target=/ic" --mount "source=$cache_dir,target=/cache" \
    --workdir /ic "$image_reference" /bin/bash /cache/build-ledger.sh
python3 scripts/audit_icrc_ledger_artifact.py --rebuilt-wasm "$cache_dir/ic-icrc1-ledger.rebuilt.wasm"
