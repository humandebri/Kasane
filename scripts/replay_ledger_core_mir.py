#!/usr/bin/env python3
"""Replay one fixed Bazel rustc action with MIR output; no semantics theorem."""
import json
import os
from pathlib import Path
import subprocess
import sys


def prepare(action):
    args = action["arguments"].copy()
    if action["mnemonic"] != "Rustc" or args.count("--") != 1:
        raise ValueError("Unexpected Rustc wrapper action")
    sep = args.index("--")
    if args[sep + 2] != "rs/ledger_suite/common/ledger_core/src/lib.rs":
        raise ValueError("Unexpected source root")
    if "--target=wasm32-unknown-unknown" not in args:
        raise ValueError("Unexpected compilation target")
    emits = [i for i, a in enumerate(args) if a.startswith("--emit=")]
    outs = [i for i, a in enumerate(args) if a.startswith("--out-dir=")]
    if len(emits) != 1 or len(outs) != 1 or args[emits[0]] != "--emit=dep-info,link":
        raise ValueError("Unexpected output options")
    args[emits[0]] = "--emit=mir=/cache/ledger-core-release-wasm32.mir"
    args[outs[0]] = "--out-dir=/cache"
    return args, args[sep + 1]


def main():
    actions = json.loads(Path(sys.argv[1]).read_text())["actions"]
    if len(actions) != 1:
        raise ValueError("Expected exactly one ledger_core Rustc action")
    args, compiler = prepare(actions[0])
    env = os.environ.copy()
    env.update({e["key"]: e.get("value", "").replace("${pwd}", os.getcwd())
                for e in actions[0]["environmentVariables"]})
    version = subprocess.check_output([compiler, "-vV"], env=env, text=True)
    Path("/cache/ledger-core-release-rustc-version.txt").write_text(version)
    if "rustc 1.93.1" not in version:
        raise ValueError("Unexpected actual Bazel compiler")
    Path("/cache/ledger-core-mir-replayed-action.json").write_text(json.dumps({
        "arguments": args, "environmentVariables": actions[0]["environmentVariables"],
        "changes": ["emit dep-info,link -> mir", "out-dir -> task cache"],
        "semantic_equivalence_proved": False}, indent=2) + "\n")
    subprocess.run(args, env=env, check=True)
    if Path("/cache/ledger-core-release-wasm32.mir").stat().st_size == 0:
        raise ValueError("Missing MIR output")


if __name__ == "__main__":
    main()
