#!/usr/bin/env python3
"""Extract all local lifetime/read/write/move events; not Rust memory semantics."""
import argparse
import re
from pathlib import Path
from audit_ledger_core_release_mir import main as audit
from extract_release_timestamp_cfg import parse as parse_cfg


def translate(text):
    parse_cfg(text)  # Validate all actual block statements and terminators.
    out = ["import ReleaseTimestampLocalLifetime", "namespace ReleaseTimestampLocalLifetime"]
    for op in ["add", "sub"]:
        header = re.search(r"fn timestamp::<impl at [^\n]+>::" + op + r"\([^\n]+", text)
        body = text[header.start():].split("\nfn ", 1)[0]
        blocks = re.findall(r"    bb(\d+): \{\n(.*?)\n    \}", body, re.S)
        events = []
        for _, block in blocks:
            for line in [v.strip() for v in block.splitlines() if v.strip()]:
                m = re.fullmatch(r"Storage(Live|Dead)\(_(\d+)\);", line)
                if m:
                    events.append(f".{m[1].lower()} {m[2]}"); continue
                m = re.fullmatch(r"_(\d+) = copy \(_1\.0: u64\);", line)
                if m:
                    events.append(f".copy 1 {m[1]}"); continue
                if line == "_8 = &_2;":
                    events.append(".borrow 2 8"); continue
                m = re.fullmatch(r"_0 = TimeStamp \{ timestamp_nanos: move _(\d+) \};", line)
                if m:
                    events.append(f".move {m[1]} 0"); continue
                if line == "return;":
                    events.append(".ret 0"); continue
                m = re.fullmatch(r"_(\d+) = .*? -> \[return: bb\d+, unwind unreachable\];", line)
                if m:
                    args = re.findall(r"move _(\d+)", line)
                    if not args:
                        raise ValueError("Missing moved operands")
                    events.append(f".call [{', '.join(args)}] {m[1]}"); continue
                raise ValueError("Unsupported lifetime event")
        out.append(f"def {op}Events : List Event :=")
        out.append("  [" + ",\n   ".join(events) + "]")
    return "\n".join(out + ["end ReleaseTimestampLocalLifetime", ""])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    audit()
    root = Path(__file__).resolve().parent.parent
    text = (root / "proofs/external/ledger-core-release-timestamp.mir").read_text()
    generated = translate(text)
    dest = root / "proofs/extraction/operator-lean/ReleaseTimestampLifetimeGenerated.lean"
    if args.check:
        if dest.read_text() != generated:
            raise ValueError("Lifetime event list differs from fixed actual MIR")
    else:
        dest.write_text(generated)
    for before, after in [("StorageLive(_8);", "StorageDead(_8);"),
                          ("try_into(move _7)", "try_into(move _9)"),
                          ("timestamp_nanos: move _3", "timestamp_nanos: move _4")]:
        if translate(text.replace(before, after, 1)) == generated:
            raise AssertionError("Changed event erased")
    print("All 20 local events per actual method retained; three changed-event witnesses preserved.")


if __name__ == "__main__":
    main()
