#!/usr/bin/env bash
# where: CI guard / what: hidden Unicode controls / why: security-sensitive source must not hide control chars
set -euo pipefail

python3 - "$@" <<'PY'
from pathlib import Path
import sys
import unicodedata

ROOTS = [
    "crates",
    "docs",
    "scripts",
    "tools",
    "README.md",
    "Cargo.toml",
    "Cargo.lock",
]
SKIP_DIRS = {
    ".git",
    ".canbench-tools",
    "node_modules",
    "out",
    "target",
    "vendor",
}
TEXT_SUFFIXES = {
    ".did",
    ".json",
    ".jsonc",
    ".md",
    ".mjs",
    ".rs",
    ".sh",
    ".sol",
    ".toml",
    ".ts",
    ".tsx",
    ".txt",
    ".yaml",
    ".yml",
}
BIDI_CONTROL = {
    0x061C,
    0x200E,
    0x200F,
    0x202A,
    0x202B,
    0x202C,
    0x202D,
    0x202E,
    0x2066,
    0x2067,
    0x2068,
    0x2069,
}


def is_control(char: str) -> bool:
    return ord(char) in BIDI_CONTROL or unicodedata.category(char) == "Cf"


def iter_files(root: Path):
    if root.is_file():
        if root.suffix in TEXT_SUFFIXES:
            yield root
        return
    for path in root.rglob("*"):
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        if path.is_file() and path.suffix in TEXT_SUFFIXES:
            yield path


failures = []
for raw_root in ROOTS:
    root = Path(raw_root)
    if not root.exists():
        continue
    for path in iter_files(root):
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        for line_no, line in enumerate(text.splitlines(), 1):
            for col_no, char in enumerate(line, 1):
                if not is_control(char):
                    continue
                code = f"U+{ord(char):04X}"
                name = unicodedata.name(char, "UNKNOWN")
                failures.append(f"{path}:{line_no}:{col_no}: {code} {name}")

if failures:
    print("[hidden-unicode] control characters found", file=sys.stderr)
    for item in failures:
        print(item, file=sys.stderr)
    sys.exit(1)

print("[hidden-unicode] ok: no Bidi_Control or Cf characters")
PY
