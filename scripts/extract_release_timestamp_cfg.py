#!/usr/bin/env python3
"""Restricted CFG witnesses; Rust lifetime/layout/MIR parser refinement unproved."""
import argparse
import re
from pathlib import Path
from audit_ledger_core_release_mir import main as audit


def parse(text):
    graphs = {}
    for op in ["add", "sub"]:
        header = re.search(r"fn timestamp::<impl at [^\n]+>::" + op + r"\([^\n]+", text)
        if header is None:
            raise ValueError("Missing method")
        body = text[header.start():].split("\nfn ", 1)[0]
        blocks = re.findall(r"    bb(\d+): \{\n(.*?)\n    \}", body, re.S)
        if [int(i) for i, _ in blocks] != list(range(5)):
            raise ValueError("Expected five unique ordered blocks")
        graph = []
        for pc, block in blocks:
            code, ignored, successor, terminated = [], [], None, False
            for line in [v.strip() for v in block.splitlines() if v.strip()]:
                if terminated:
                    raise ValueError("Statement after terminator")
                if re.fullmatch(r"Storage(?:Live|Dead)\(_\d+\);", line) or line == "_8 = &_2;":
                    ignored.append(line)
                    continue
                match = re.fullmatch(r"_(\d+) = copy \(_1\.0: u64\);", line)
                if match:
                    code.append(".input " + match[1]); continue
                match = re.fullmatch(r"_0 = TimeStamp \{ timestamp_nanos: move _(\d+) \};", line)
                if match:
                    code.append(".returnTimestamp " + match[1]); continue
                if line == "return;":
                    if len(code) != 1 or not code[0].startswith(".returnTimestamp "):
                        raise ValueError("Unsupported return")
                    terminated = True; continue
                match = re.fullmatch(r"_(\d+) = (.*?) -> \[return: bb(\d+), unwind unreachable\];", line)
                if match is None:
                    raise ValueError("Unsupported block statement: " + line)
                dst, call, successor = match[1], match[2], int(match[3])
                if not 0 <= successor < 5:
                    raise ValueError("Out-of-range successor")
                args = re.fullmatch(r"Duration::as_nanos\(move _8\)", call)
                if args:
                    code.append(".asNanos " + dst)
                else:
                    args = re.fullmatch(r"<u128 as TryInto<u64>>::try_into\(move _(\d+)\)", call)
                    if args:
                        code.append(f".narrow {dst} {args[1]}")
                    else:
                        args = re.fullmatch(r"Result::<u64, std::num::TryFromIntError>::unwrap\(move _(\d+)\)", call)
                        if args:
                            code.append(f".unwrap {dst} {args[1]}")
                        else:
                            args = re.fullmatch(r"core::num::<impl u64>::saturating_" + op + r"\(move _(\d+), move _(\d+)\)", call)
                            if args is None:
                                raise ValueError("Unsupported call")
                            code.append(f".saturate .{op} {dst} {args[1]} {args[2]}")
                terminated = True
            if not terminated:
                raise ValueError("Missing block terminator")
            graph.append((int(pc), code, successor, ignored))
        graphs[op] = graph
    return graphs


def translate(text):
    out = ["import ReleaseTimestampCFG", "namespace ReleaseTimestampCFG"]
    for op, graph in parse(text).items():
        out.append(f"def release{op.title()}CFG : Graph")
        for pc, code, successor, _ in graph:
            next_pc = "none" if successor is None else f"some {successor}"
            out.append(f"  | {pc} => some ⟨[{', '.join(code)}], {next_pc}⟩")
        out.append("  | _ => none")
    return "\n".join(out + ["end ReleaseTimestampCFG", ""])


def main():
    args = argparse.ArgumentParser()
    args.add_argument("--check", action="store_true")
    opts = args.parse_args()
    audit()
    root = Path(__file__).resolve().parent.parent
    text = (root / "proofs/external/ledger-core-release-timestamp.mir").read_text()
    generated = translate(text)
    dest = root / "proofs/extraction/operator-lean/ReleaseTimestampCFGGenerated.lean"
    if opts.check:
        if dest.read_text() != generated:
            raise ValueError("Generated CFG differs from fixed actual MIR")
    else:
        dest.write_text(generated)
    mutations = [text.replace("unwind unreachable", "unwind continue", 1),
                 text.replace("return: bb1", "return: bb9", 1),
                 text.replace("StorageDead(_8);", "arbitrary_effect();", 1),
                 text.replace("    bb1:", "    bb0:", 1),
                 text.replace("return: bb1, unwind unreachable];", "return: bb1, unwind unreachable];\n        arbitrary_effect();", 1)]
    for bad in mutations:
        try:
            parse(bad)
        except ValueError:
            pass
        else:
            raise AssertionError("Unsupported CFG mutation accepted")
    changed = text.replace("return: bb1", "return: bb2", 1)
    if translate(changed) == generated:
        raise AssertionError("Changed valid successor erased")
    print("Two actual five-block CFG witnesses mapped; five unsupported mutations rejected; a valid changed successor retained.")
    print("Storage/reference/move/unwind and full Rust MIR semantics remain abstraction obligations.")


if __name__ == "__main__":
    main()
