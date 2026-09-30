#!/usr/bin/env python3
"""Fail on changes to the reviewed revm source tree or production feature graph."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
PROFILE = ROOT / "proofs/evm/revm-profile.json"


def current_profile():
    digest = hashlib.sha256()
    paths = sorted(
        p for p in (ROOT / "vendor/revm").rglob("*")
        if p.is_file() and (p.suffix in {".rs", ".toml"} or p.name == "Cargo.lock")
        and "target" not in p.relative_to(ROOT / "vendor/revm").parts
    )
    for path in paths:
        digest.update(path.relative_to(ROOT).as_posix().encode() + b"\0")
        digest.update(hashlib.sha256(path.read_bytes()).digest())
    provenance = (ROOT / "vendor/revm/VENDORED_FROM.md").read_text()
    source = next(line.removeprefix("Source: ") for line in provenance.splitlines() if line.startswith("Source: "))
    commit = next(line.removeprefix("Pinned commit: ") for line in provenance.splitlines() if line.startswith("Pinned commit: "))
    output = subprocess.check_output([
        "cargo", "tree", "--locked", "--offline", "-p", "ic-evm-gateway",
        "--target", "wasm32-unknown-unknown", "-e", "normal,build",
        "--prefix", "none", "--format", "{p} features=[{f}]",
    ], cwd=ROOT, text=True)
    features = sorted({
        line.replace(str(ROOT) + "/", "").removesuffix(" (*)")
        for line in output.splitlines() if line.startswith("revm")
    })
    if not paths or not features:
        raise RuntimeError("Empty revm source or feature inventory")
    return {
        "source": source, "declared_commit": commit,
        "source_files": len(paths), "source_tree_sha256": digest.hexdigest(),
        "target": "wasm32-unknown-unknown", "package": "ic-evm-gateway",
        "features": features,
    }


if __name__ == "__main__":
    actual = current_profile()
    if sys.argv[1:] == ["--print-current"]:
        print(json.dumps(actual, indent=2))
    elif sys.argv[1:]:
        raise SystemExit("usage: check_revm_verification_profile.py [--print-current]")
    elif actual != json.loads(PROFILE.read_text()):
        raise SystemExit("revm profile changed: review source, fork, features and proofs before updating proofs/evm/revm-profile.json")
    else:
        print("[revm-profile] vendored source and Wasm production features match")
