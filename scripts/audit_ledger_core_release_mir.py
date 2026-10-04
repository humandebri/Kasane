#!/usr/bin/env python3
"""Audit fixed actual compiler-action/MIR witnesses, not MIR semantics."""
import copy
import hashlib
import json
from pathlib import Path
from replay_ledger_core_mir import prepare


def check_routes(text):
    expected = [
        "Duration::as_nanos(move _8) -> [return: bb1, unwind unreachable]",
        "<u128 as TryInto<u64>>::try_into(move _7) -> [return: bb2, unwind unreachable]",
        "Result::<u64, std::num::TryFromIntError>::unwrap(move _6) -> [return: bb3, unwind unreachable]",
    ]
    for line, op in [(65, "add"), (77, "sub")]:
        marker = f"fn timestamp::<impl at rs/ledger_suite/common/ledger_core/src/timestamp.rs:{line}:1: {line}:33>::{op}("
        if text.count(marker) != 1:
            raise ValueError("Missing or duplicate actual timestamp method")
        body = text[text.index(marker):].split("\nfn ", 1)[0]
        routes = expected + [f"core::num::<impl u64>::saturating_{op}(move _4, move _5) -> [return: bb4, unwind unreachable]"]
        positions = []
        for route in routes:
            if body.count(route) != 1:
                raise ValueError("Actual timestamp call route changed")
            positions.append(body.index(route))
        if positions != sorted(positions) or "_0 = TimeStamp { timestamp_nanos: move _3 };" not in body:
            raise ValueError("Actual timestamp call order/result changed")


def main():
    root = Path(__file__).resolve().parent.parent
    profile = json.loads((root / "proofs/external/ledger-core-release-mir-profile.json").read_text())
    for filename, digest in profile["sha256"].items():
        if hashlib.sha256((root / filename).read_bytes()).hexdigest() != digest:
            raise ValueError("Fixed witness changed: " + filename)
    cache = root / ".local/proof-tools/ledger-repro-cache"
    action = json.loads((cache / "ledger-core-rustc-actions.json").read_text())["actions"][0]
    replay = json.loads((cache / "ledger-core-mir-replayed-action.json").read_text())
    args, _ = prepare(action)
    assert args == replay["arguments"]
    assert action["environmentVariables"] == replay["environmentVariables"]
    assert sum(a != b for a, b in zip(action["arguments"], args)) == 2
    full = (cache / "ledger-core-release-wasm32.mir").read_text()
    snapshot = (root / "proofs/external/ledger-core-release-timestamp.mir").read_text()
    check_routes(full)
    check_routes(snapshot)
    for body in snapshot.split("\n\nfn "):
        body = body if body.startswith("fn ") else "fn " + body
        assert body.rstrip() in full
    for field in ["target", "source", "emit", "duplicate_emit"]:
        bad = copy.deepcopy(action)
        if field == "duplicate_emit":
            bad["arguments"].append("--emit=mir")
        else:
            before, after = {"target": ("--target=wasm32-unknown-unknown", "--target=aarch64-apple-darwin"),
                             "source": ("rs/ledger_suite/common/ledger_core/src/lib.rs", "modified.rs"),
                             "emit": ("--emit=dep-info,link", "--emit=mir")}[field]
            bad["arguments"] = [v.replace(before, after) for v in bad["arguments"]]
        try:
            prepare(bad)
        except ValueError:
            pass
        else:
            raise AssertionError("Changed compiler action accepted")
    mutations = [snapshot.replace("saturating_add(move", "saturating_sub(move"),
                 snapshot.replace("::unwrap(move _6)", "::unwrap_or_default(move _6)"),
                 snapshot.replace("return: bb2", "return: bb4"),
                 snapshot.replace("timestamp_nanos: move _3", "timestamp_nanos: move _4")]
    for bad in mutations:
        try:
            check_routes(bad)
        except ValueError:
            pass
        else:
            raise AssertionError("Changed actual MIR route accepted")
    print("Actual timestamp add/sub MIR routes verified; two output-only action changes; eight mutations rejected.")
    print("MIR semantics/std-library/LLVM/Wasm/IC equivalence remains unproved.")


if __name__ == "__main__":
    main()
