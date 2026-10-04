#!/usr/bin/env python3
"""Audit fixed artifact identity metadata; does not prove Wasm/IC semantics."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def custom_sections(data: bytes) -> dict[str, list[bytes]]:
    if data[:8] != b'\0asm\1\0\0\0':
        raise ValueError('invalid Wasm header')

    def u32(pos: int, limit: int) -> tuple[int, int]:
        value = 0
        for shift in range(0, 35, 7):
            if pos >= limit:
                raise ValueError('truncated section length')
            byte = data[pos]
            pos += 1
            if shift == 28 and byte & 0xF0:
                raise ValueError('oversized section length')
            value |= (byte & 0x7F) << shift
            if byte < 0x80:
                return value, pos
        raise ValueError('oversized section length')

    sections: dict[str, list[bytes]] = {}
    pos = 8
    while pos < len(data):
        kind = data[pos]
        size, pos = u32(pos + 1, len(data))
        end = pos + size
        if end > len(data):
            raise ValueError('truncated section payload')
        if kind == 0:
            length, start = u32(pos, end)
            if start + length > end:
                raise ValueError('truncated custom section name')
            name = data[start:start + length].decode('utf-8')
            sections.setdefault(name, []).append(data[start + length:end])
        pos = end
    return sections


def audit_metadata(data: bytes, commit: str, candid: bytes) -> None:
    sections = custom_sections(data)
    if sections.get('icp:public git_commit_id') != [(commit + '\n').encode()]:
        raise ValueError('embedded commit missing, duplicate, or mismatched')
    if sections.get('icp:public candid:service') != [candid]:
        raise ValueError('embedded Candid missing, duplicate, or mismatched')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-root', type=Path)
    parser.add_argument('--rebuilt-wasm', type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    profile = json.loads((root / 'proofs/external/ledger-profile.json').read_text())
    source = args.source_root or root / '.local/proof-tools/ic-ledger-source'
    head = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
    if head != profile['source_commit']:
        raise ValueError('source checkout commit mismatch')
    for item in profile['reviewed_sources']:
        if hashlib.sha256((source / item['path']).read_bytes()).hexdigest() != item['sha256']:
            raise ValueError('reviewed source mismatch: ' + item['path'])
    data = (root / profile['bundled_wasm']).read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if digest != profile['wasm_sha256']:
        raise ValueError('bundled artifact hash mismatch')
    candid = (source / 'rs/ledger_suite/icrc1/ledger/ledger.did').read_bytes()
    audit_metadata(data, head, candid)
    rebuilt = None
    if args.rebuilt_wasm:
        other = args.rebuilt_wasm.read_bytes()
        if other != data:
            raise ValueError('rebuilt artifact bytes differ from bundled Wasm')
        rebuilt = True
    print(json.dumps({'source_commit': head, 'wasm_sha256': digest,
                      'embedded_commit_matches': True, 'candid_bytes_equal': True,
                      'rebuild_bytes_equal': rebuilt, 'semantic_equivalence_proved': False}, indent=2))


if __name__ == '__main__':
    main()
