#!/usr/bin/env python3
"""Restricted fixed-MIR witness translation, not a verified rustc/MIR frontend."""
import argparse
import re
from pathlib import Path
from audit_ledger_core_release_mir import main as audit


def translate(text):
    output = ["import ReleaseTimestampSequence", "namespace ReleaseTimestampSequence"]
    for op in ["add", "sub"]:
        marker = re.search(r"fn timestamp::<impl at [^\n]+>::" + op + r"\([^\n]+", text)
        if marker is None:
            raise ValueError("Missing method")
        body = text[marker.start():].split("\nfn ", 1)[0]
        patterns = [r"_(\d+) = copy \(_1\.0: u64\);",
                    r"_(\d+) = Duration::as_nanos\(move _8\)",
                    r"_(\d+) = <u128 as TryInto<u64>>::try_into\(move _(\d+)\)",
                    r"_(\d+) = Result::<u64, std::num::TryFromIntError>::unwrap\(move _(\d+)\)",
                    r"_(\d+) = core::num::<impl u64>::saturating_" + op + r"\(move _(\d+), move _(\d+)\)",
                    r"_0 = TimeStamp \{ timestamp_nanos: move _(\d+) \};"]
        found = [re.findall(pattern, body) for pattern in patterns]
        if any(len(matches) != 1 for matches in found):
            raise ValueError("Unsupported MIR register mapping")
        a, b, c, d, e, f = [matches[0] for matches in found]
        output += [f"def release{op.title()} : List Statement :=", 
                   f"  [.input {a}, .asNanos {b}, .narrow {c[0]} {c[1]}, .unwrap {d[0]} {d[1]},",
                   f"   .saturate .{op} {e[0]} {e[1]} {e[2]}, .returnTimestamp {f}]"]
    return "\n".join(output + ["end ReleaseTimestampSequence", ""])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    audit()  # Fixed SHA/exact stored body/call route and output-only action changes.
    root = Path(__file__).resolve().parent.parent
    text = (root / "proofs/external/ledger-core-release-timestamp.mir").read_text()
    generated = translate(text)
    dest = root / "proofs/extraction/operator-lean/ReleaseTimestampGenerated.lean"
    if args.check:
        if dest.read_text() != generated:
            raise ValueError("Generated program differs from actual fixed MIR")
    else:
        dest.write_text(generated)
    print("Two fixed actual MIR call-sequence witnesses mapped to Lean register programs; frontend refinement remains unproved.")


if __name__ == "__main__":
    main()
